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
    VNHumanHandPoseObservationJointNameThumbTip, VNImageRequestHandler, VNRecognizedPoint, VNRequest, VNDetectBarcodesRequest, VNRecognizeTextRequest,
    VNRequestTextRecognitionLevel,
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
        // Hands and bodies need macOS 11 (older Macs track faces only).
        let pose = crate::os::macos_at_least(11, 0);
        let hands = (cfg.hands && pose).then(|| {
            let r = VNDetectHumanHandPoseRequest::new();
            r.setMaximumHandCount(2);
            r
        });
        let body = (cfg.body && pose).then(|| VNDetectHumanBodyPoseRequest::new());
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

/// QR codes / barcodes and text in an RGBA picture: (the biggest code's contents, every line of text read, top to
/// bottom), for what was asked.
pub fn read(rgba: &[u8], width: u32, height: u32, codes: bool, text: bool) -> Result<(Option<String>, Option<String>), String> {
    if rgba.len() < (width * height * 4) as usize {
        return Err("Reading: picture too small".into());
    }
    autoreleasepool(|_| unsafe {
        let pixels = pixel_buffer(rgba, width, height).ok_or("Reading: no picture buffer")?;
        let handler = VNImageRequestHandler::initWithCVPixelBuffer_options(VNImageRequestHandler::alloc(), &pixels, &NSDictionary::new());
        let code_req = codes.then(|| VNDetectBarcodesRequest::new());
        let text_req = text.then(|| {
            let r = VNRecognizeTextRequest::new();
            r.setRecognitionLevel(VNRequestTextRecognitionLevel::Fast);
            r.setUsesLanguageCorrection(true);
            r
        });
        let mut requests: Vec<&VNRequest> = Vec::new();
        if let Some(r) = &code_req {
            requests.push(r);
        }
        if let Some(r) = &text_req {
            requests.push(r);
        }
        handler.performRequests_error(&NSArray::from_slice(&requests)).map_err(|e| format!("Reading: {}", e.localizedDescription()))?;
        let code = code_req.and_then(|r| {
            r.results()?
                .iter()
                .filter_map(|o| Some((area(o.boundingBox()), o.payloadStringValue()?.to_string())))
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, s)| s)
        });
        let read = text_req.and_then(|r| {
            let mut lines: Vec<(f64, String)> = r
                .results()?
                .iter()
                .filter_map(|o| {
                    let best = o.topCandidates(1).firstObject()?;
                    Some((o.boundingBox().origin.y, best.string().to_string()))
                })
                .collect();
            // Vision's y counts up: highest first reads top to bottom.
            lines.sort_by(|a, b| b.0.total_cmp(&a.0));
            let all = lines.into_iter().map(|(_, s)| s).collect::<Vec<_>>().join("\n");
            (!all.is_empty()).then_some(all)
        });
        Ok((code, read))
    })
}

/// Where the people are in an RGBA picture (Vision person segmentation, balanced quality).
pub fn person_matte(rgba: &[u8], width: u32, height: u32) -> Result<Matte, String> {
    if rgba.len() < (width * height * 4) as usize {
        return Err("Remove background: picture too small".into());
    }
    if !crate::os::macos_at_least(12, 0) {
        return Err("Remove background needs macOS 12 or later".into());
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

#[cfg(test)]
mod read_tests {
    /// A QR code drawn by Phosphor's own generator is read back by Vision.
    #[test]
    fn reads_a_qr_code() {
        let qr = crate::ui::qr_image("https://phosphorfx.com/show/42", 320, [0, 0, 0, 255], [255, 255, 255, 255]).expect("a code");
        let (code, _) = super::read(qr.as_raw(), qr.width(), qr.height(), true, false).expect("Vision ran");
        assert_eq!(code.as_deref(), Some("https://phosphorfx.com/show/42"));
    }

    /// Words drawn by Phosphor's text layer are read back.
    #[test]
    fn reads_text() {
        let style = phosphor_engine::TextStyle { text: "HELLO PHOSPHOR".into(), size: 0.2, color: [0, 0, 0, 255], ..Default::default() };
        let img = crate::text::render(&style, 960, 540).expect("drawn");
        // White behind the black words.
        let mut px = img.into_raw();
        for p in px.chunks_mut(4) {
            let a = p[3] as f32 / 255.0;
            for c in &mut p[..3] {
                *c = (*c as f32 * a + 255.0 * (1.0 - a)) as u8;
            }
            p[3] = 255;
        }
        let (_, text) = super::read(&px, 960, 540, false, true).expect("Vision ran");
        assert_eq!(text.as_deref().map(str::to_uppercase).as_deref(), Some("HELLO PHOSPHOR"), "{text:?}");
    }
}
