//! Apple Vision: faces, hand pose and body pose in one RGBA picture.

use std::ptr::NonNull;

use objc2::AllocAnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA, kCVPixelFormatType_OneComponent8,
    kCVReturnSuccess,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{
    VNDetectFaceRectanglesRequest, VNDetectHumanBodyPoseRequest, VNDetectHumanHandPoseRequest, VNGeneratePersonSegmentationRequest,
    VNGeneratePersonSegmentationRequestQualityLevel, VNHumanBodyPoseObservationJointNameLeftWrist,
    VNHumanBodyPoseObservationJointNameRightWrist, VNHumanBodyPoseObservationJointNameRoot, VNHumanHandPoseObservationJointNameIndexTip,
    VNHumanHandPoseObservationJointNameThumbTip, VNImageRequestHandler, VNRecognizedPoint, VNRequest,
};

use super::{Found, Matte};

/// Points Vision is less sure of than this don't count.
const SURE: f32 = 0.3;

pub fn look(rgba: &[u8], width: u32, height: u32, cfg: &phosphor_engine::Tracking) -> Result<Found, String> {
    if rgba.len() < (width * height * 4) as usize {
        return Err("Tracking: picture too small".into());
    }
    autoreleasepool(|_| unsafe {
        let pixels = pixel_buffer(rgba, width, height).ok_or("Tracking: no picture buffer")?;
        let handler = VNImageRequestHandler::initWithCVPixelBuffer_options(VNImageRequestHandler::alloc(), &pixels, &NSDictionary::new());
        let faces = cfg.faces.then(|| VNDetectFaceRectanglesRequest::new());
        let hands = cfg.hands.then(|| {
            let r = VNDetectHumanHandPoseRequest::new();
            r.setMaximumHandCount(2);
            r
        });
        let body = cfg.body.then(|| VNDetectHumanBodyPoseRequest::new());
        let mut requests: Vec<&VNRequest> = Vec::new();
        if let Some(r) = &faces {
            requests.push(r);
        }
        if let Some(r) = &hands {
            requests.push(r);
        }
        if let Some(r) = &body {
            requests.push(r);
        }
        handler.performRequests_error(&NSArray::from_slice(&requests)).map_err(|e| format!("Tracking: {}", e.localizedDescription()))?;
        let mut found = Found::new();
        // The biggest face: its centre (Vision's y counts up from the bottom, like the XY pads) and height.
        if let Some(r) = &faces
            && let Some(face) = r.results().and_then(|list| list.iter().max_by(|a, b| area(a.boundingBox()).total_cmp(&area(b.boundingBox()))))
        {
            let b = face.boundingBox();
            found.push(("Track Face X", (b.origin.x + b.size.width / 2.0) as f32));
            found.push(("Track Face Y", (b.origin.y + b.size.height / 2.0) as f32));
            found.push(("Track Face Size", b.size.height as f32));
            found.push(("Track Face Found", 1.0));
        }
        if let Some(r) = &hands
            && let Some(hand) = r.results().and_then(|list| list.firstObject())
        {
            let tip = sure(hand.recognizedPointForJointName_error(VNHumanHandPoseObservationJointNameIndexTip).ok());
            let thumb = sure(hand.recognizedPointForJointName_error(VNHumanHandPoseObservationJointNameThumbTip).ok());
            if let Some(t) = tip {
                found.push(("Track Hand X", t[0]));
                found.push(("Track Hand Y", t[1]));
                found.push(("Track Hand Found", 1.0));
                if let Some(th) = thumb {
                    // A quarter of the picture apart = fully open.
                    found.push(("Track Pinch", ((t[0] - th[0]).hypot(t[1] - th[1]) / 0.25).min(1.0)));
                }
            }
        }
        if let Some(r) = &body
            && let Some(person) = r.results().and_then(|list| list.firstObject())
        {
            let joint = |name| sure(person.recognizedPointForJointName_error(name).ok());
            if let Some(root) = joint(VNHumanBodyPoseObservationJointNameRoot) {
                found.push(("Track Body X", root[0]));
                found.push(("Track Body Y", root[1]));
                found.push(("Track Body Found", 1.0));
            }
            if let Some(w) = joint(VNHumanBodyPoseObservationJointNameLeftWrist) {
                found.push(("Track Left Wrist Y", w[1]));
            }
            if let Some(w) = joint(VNHumanBodyPoseObservationJointNameRightWrist) {
                found.push(("Track Right Wrist Y", w[1]));
            }
        }
        Ok(found)
    })
}

/// Where the people are in an RGBA picture (Vision person segmentation, balanced quality).
pub fn person_matte(rgba: &[u8], width: u32, height: u32) -> Result<Matte, String> {
    if rgba.len() < (width * height * 4) as usize {
        return Err("Remove background: picture too small".into());
    }
    autoreleasepool(|_| unsafe {
        let pixels = pixel_buffer(rgba, width, height).ok_or("Remove background: no picture buffer")?;
        let handler = VNImageRequestHandler::initWithCVPixelBuffer_options(VNImageRequestHandler::alloc(), &pixels, &NSDictionary::new());
        let request = VNGeneratePersonSegmentationRequest::new();
        request.setQualityLevel(VNGeneratePersonSegmentationRequestQualityLevel::Balanced);
        request.setOutputPixelFormat(kCVPixelFormatType_OneComponent8);
        let requests: [&VNRequest; 1] = [&request];
        handler.performRequests_error(&NSArray::from_slice(&requests)).map_err(|e| format!("Remove background: {}", e.localizedDescription()))?;
        let Some(result) = request.results().and_then(|r| r.firstObject()) else { return Ok((1, 1, vec![0])) };
        let mask = result.pixelBuffer();
        let lock = CVPixelBufferLockFlags::ReadOnly;
        CVPixelBufferLockBaseAddress(&mask, lock);
        let (w, h, stride) = (CVPixelBufferGetWidth(&mask), CVPixelBufferGetHeight(&mask), CVPixelBufferGetBytesPerRow(&mask));
        let base = CVPixelBufferGetBaseAddress(&mask) as *const u8;
        let out = if base.is_null() {
            Vec::new()
        } else {
            (0..h).flat_map(|y| std::slice::from_raw_parts(base.add(y * stride), w).to_vec()).collect()
        };
        CVPixelBufferUnlockBaseAddress(&mask, lock);
        if out.is_empty() {
            return Err("Remove background: empty matte".into());
        }
        Ok((w as u32, h as u32, out))
    })
}

fn area(r: objc2_core_foundation::CGRect) -> f64 {
    r.size.width * r.size.height
}

/// A recognised point's (x, y) when Vision is sure enough of it.
unsafe fn sure(p: Option<Retained<VNRecognizedPoint>>) -> Option<[f32; 2]> {
    let p = p?;
    if unsafe { p.confidence() } < SURE {
        return None;
    }
    let at = unsafe { p.location() };
    Some([at.x as f32, at.y as f32])
}

/// The RGBA picture as a BGRA pixel buffer (what Vision reads fastest).
unsafe fn pixel_buffer(rgba: &[u8], width: u32, height: u32) -> Option<CFRetained<CVPixelBuffer>> {
    let mut raw: *mut CVPixelBuffer = std::ptr::null_mut();
    let r = unsafe { CVPixelBufferCreate(None, width as usize, height as usize, kCVPixelFormatType_32BGRA, None, NonNull::from(&mut raw)) };
    if r != kCVReturnSuccess {
        return None;
    }
    let buffer = unsafe { CFRetained::from_raw(NonNull::new(raw)?) };
    unsafe { CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0)) };
    let base = CVPixelBufferGetBaseAddress(&buffer) as *mut u8;
    let stride = CVPixelBufferGetBytesPerRow(&buffer);
    if !base.is_null() {
        let row = width as usize * 4;
        for y in 0..height as usize {
            let dst = unsafe { std::slice::from_raw_parts_mut(base.add(y * stride), row) };
            for (d, s) in dst.chunks_exact_mut(4).zip(rgba[y * row..(y + 1) * row].chunks_exact(4)) {
                d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
            }
        }
    }
    unsafe { CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0)) };
    (!base.is_null()).then_some(buffer)
}
