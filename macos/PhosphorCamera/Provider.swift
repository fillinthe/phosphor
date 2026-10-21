import CoreMediaIO
import Foundation
import IOKit.audio

/// The picture size and rate the camera offers.
let cameraWidth: Int32 = 1920
let cameraHeight: Int32 = 1080
let cameraFps: Int32 = 30

final class PhosphorProviderSource: NSObject, CMIOExtensionProviderSource {
    private(set) var provider: CMIOExtensionProvider!
    private var deviceSource: PhosphorDeviceSource!

    init(clientQueue: DispatchQueue?) {
        super.init()
        provider = CMIOExtensionProvider(source: self, clientQueue: clientQueue)
        deviceSource = PhosphorDeviceSource(localizedName: "Phosphor")
        do {
            try provider.addDevice(deviceSource.device)
        } catch {
            fatalError("Phosphor camera: couldn't add the device: \(error)")
        }
    }

    func connect(to client: CMIOExtensionClient) throws {}
    func disconnect(from client: CMIOExtensionClient) {}

    var availableProperties: Set<CMIOExtensionProperty> { [.providerManufacturer] }

    func providerProperties(forProperties properties: Set<CMIOExtensionProperty>) throws -> CMIOExtensionProviderProperties {
        let p = CMIOExtensionProviderProperties(dictionary: [:])
        if properties.contains(.providerManufacturer) {
            p.manufacturer = "Phosphor"
        }
        return p
    }

    func setProviderProperties(_ providerProperties: CMIOExtensionProviderProperties) throws {}
}

/// The "Phosphor" camera: a source stream apps read, and a sink stream Phosphor writes into.
final class PhosphorDeviceSource: NSObject, CMIOExtensionDeviceSource {
    private(set) var device: CMIOExtensionDevice!
    private var sourceStream: PhosphorStreamSource!
    private var sinkStream: PhosphorStreamSource!
    private(set) var format: CMFormatDescription!
    private var readers = 0
    private var blackTimer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "phosphor.camera")
    private var lastFromPhosphor: UInt64 = 0
    private var sinkClient: CMIOExtensionClient?

    init(localizedName: String) {
        super.init()
        let deviceID = UUID(uuidString: "6E0C4B5A-2D3F-4A8B-9C1E-7F3D2B1A0C5E")!
        device = CMIOExtensionDevice(localizedName: localizedName, deviceID: deviceID, legacyDeviceID: nil, source: self)
        CMVideoFormatDescriptionCreate(allocator: kCFAllocatorDefault, codecType: kCVPixelFormatType_32BGRA, width: cameraWidth, height: cameraHeight, extensions: nil, formatDescriptionOut: &format)
        let streamFormat = CMIOExtensionStreamFormat(formatDescription: format, maxFrameDuration: CMTime(value: 1, timescale: cameraFps), minFrameDuration: CMTime(value: 1, timescale: cameraFps), validFrameDurations: nil)
        sourceStream = PhosphorStreamSource(localizedName: "Phosphor", streamID: UUID(uuidString: "3B9F1C2D-4E5A-4B6C-8D7E-9F0A1B2C3D4E")!, format: streamFormat, device: self, direction: .source)
        sinkStream = PhosphorStreamSource(localizedName: "Phosphor in", streamID: UUID(uuidString: "8C7D6E5F-4A3B-4C2D-9E1F-0A1B2C3D4E5F")!, format: streamFormat, device: self, direction: .sink)
        do {
            try device.addStream(sourceStream.stream)
            try device.addStream(sinkStream.stream)
        } catch {
            fatalError("Phosphor camera: couldn't add the streams: \(error)")
        }
    }

    var availableProperties: Set<CMIOExtensionProperty> { [.deviceTransportType, .deviceModel] }

    func deviceProperties(forProperties properties: Set<CMIOExtensionProperty>) throws -> CMIOExtensionDeviceProperties {
        let p = CMIOExtensionDeviceProperties(dictionary: [:])
        if properties.contains(.deviceTransportType) {
            p.transportType = kIOAudioDeviceTransportTypeVirtual
        }
        if properties.contains(.deviceModel) {
            p.model = "Phosphor"
        }
        return p
    }

    func setDeviceProperties(_ deviceProperties: CMIOExtensionDeviceProperties) throws {}

    // An app started watching: black frames until Phosphor's arrive.
    func startStreaming() {
        queue.async { [self] in
            self.readers += 1
            guard self.blackTimer == nil else { return }
            let t = DispatchSource.makeTimerSource(queue: self.queue)
            t.schedule(deadline: .now(), repeating: 1.0 / Double(cameraFps))
            t.setEventHandler { [weak self] in self?.sendBlackIfIdle() }
            self.blackTimer = t
            t.resume()
        }
    }

    func stopStreaming() {
        queue.async {
            self.readers = max(0, self.readers - 1)
            if self.readers == 0 {
                self.blackTimer?.cancel()
                self.blackTimer = nil
            }
        }
    }

    /// Phosphor connected to the sink: take its frames, one after another, and pass each on.
    func startSink(client: CMIOExtensionClient) {
        sinkClient = client
        consume()
    }

    func stopSink() {
        sinkClient = nil
    }

    private func consume() {
        guard let client = sinkClient else { return }
        sinkStream.stream.consumeSampleBuffer(from: client) { [weak self] buffer, sequence, _, _, _ in
            guard let self else { return }
            if let buffer {
                self.queue.async {
                    self.lastFromPhosphor = DispatchTime.now().uptimeNanoseconds
                    if self.readers > 0 {
                        self.sourceStream.stream.send(buffer, discontinuity: [], hostTimeInNanoseconds: UInt64(buffer.presentationTimeStamp.seconds * Double(NSEC_PER_SEC)))
                    }
                }
                let output = CMIOExtensionScheduledOutput(sequenceNumber: sequence, hostTimeInNanoseconds: DispatchTime.now().uptimeNanoseconds)
                self.sinkStream.stream.notifyScheduledOutputChanged(output)
            }
            self.consume()
        }
    }

    /// Black while Phosphor isn't sending (it stopped, or hasn't started).
    private func sendBlackIfIdle() {
        let now = DispatchTime.now().uptimeNanoseconds
        guard readers > 0, now - lastFromPhosphor > 500_000_000 else { return }
        var pixelBuffer: CVPixelBuffer?
        let attrs = [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary
        guard CVPixelBufferCreate(kCFAllocatorDefault, Int(cameraWidth), Int(cameraHeight), kCVPixelFormatType_32BGRA, attrs, &pixelBuffer) == kCVReturnSuccess, let pb = pixelBuffer else { return }
        CVPixelBufferLockBaseAddress(pb, [])
        if let base = CVPixelBufferGetBaseAddress(pb) {
            memset(base, 0, CVPixelBufferGetBytesPerRow(pb) * Int(cameraHeight))
        }
        CVPixelBufferUnlockBaseAddress(pb, [])
        var timing = CMSampleTimingInfo(duration: CMTime(value: 1, timescale: cameraFps), presentationTimeStamp: CMClockGetTime(CMClockGetHostTimeClock()), decodeTimeStamp: .invalid)
        var sample: CMSampleBuffer?
        guard CMSampleBufferCreateReadyWithImageBuffer(allocator: kCFAllocatorDefault, imageBuffer: pb, formatDescription: format, sampleTiming: &timing, sampleBufferOut: &sample) == noErr, let sample else { return }
        sourceStream.stream.send(sample, discontinuity: [], hostTimeInNanoseconds: now)
    }
}

/// One stream (source = what apps read; sink = what Phosphor writes).
final class PhosphorStreamSource: NSObject, CMIOExtensionStreamSource {
    private(set) var stream: CMIOExtensionStream!
    private let streamFormat: CMIOExtensionStreamFormat
    private unowned let deviceSource: PhosphorDeviceSource
    private let direction: CMIOExtensionStream.Direction
    /// The app allowed to start this stream (the sink: Phosphor).
    private var client: CMIOExtensionClient?

    init(localizedName: String, streamID: UUID, format: CMIOExtensionStreamFormat, device: PhosphorDeviceSource, direction: CMIOExtensionStream.Direction) {
        streamFormat = format
        deviceSource = device
        self.direction = direction
        super.init()
        stream = CMIOExtensionStream(localizedName: localizedName, streamID: streamID, direction: direction, clockType: .hostTime, source: self)
    }

    var formats: [CMIOExtensionStreamFormat] { [streamFormat] }

    var availableProperties: Set<CMIOExtensionProperty> {
        var p: Set<CMIOExtensionProperty> = [.streamActiveFormatIndex, .streamFrameDuration]
        if direction == .sink {
            p.formUnion([.streamSinkBufferQueueSize, .streamSinkBuffersRequiredForStartup, .streamSinkBufferUnderrunCount, .streamSinkEndOfData])
        }
        return p
    }

    func streamProperties(forProperties properties: Set<CMIOExtensionProperty>) throws -> CMIOExtensionStreamProperties {
        let p = CMIOExtensionStreamProperties(dictionary: [:])
        if properties.contains(.streamActiveFormatIndex) {
            p.activeFormatIndex = 0
        }
        if properties.contains(.streamFrameDuration) {
            p.frameDuration = CMTime(value: 1, timescale: cameraFps)
        }
        if properties.contains(.streamSinkBufferQueueSize) {
            p.sinkBufferQueueSize = 1
        }
        if properties.contains(.streamSinkBuffersRequiredForStartup) {
            p.sinkBuffersRequiredForStartup = 1
        }
        return p
    }

    func setStreamProperties(_ streamProperties: CMIOExtensionStreamProperties) throws {}

    func authorizedToStartStream(for client: CMIOExtensionClient) -> Bool {
        self.client = client
        return true
    }

    func startStream() throws {
        if direction == .source {
            deviceSource.startStreaming()
        } else if let client {
            deviceSource.startSink(client: client)
        }
    }

    func stopStream() throws {
        if direction == .source {
            deviceSource.stopStreaming()
        } else {
            deviceSource.stopSink()
        }
    }
}
