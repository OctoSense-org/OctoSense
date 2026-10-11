// Generate a silent synthetic H.264 fixture using the macOS SDK, with no
// camera, microphone, network input or third-party media. Does not overwrite.
// Usage: swift tools/generate-video-fixture.swift /new/path/playback.mp4
import AVFoundation
import CoreVideo
import Foundation

guard CommandLine.arguments.count == 2 else {
    fatalError("Pass a new output .mp4 path")
}
let output = URL(fileURLWithPath: CommandLine.arguments[1])
guard !FileManager.default.fileExists(atPath: output.path) else {
    fatalError("Refusing to overwrite output")
}
let width = 320, height = 180, frames = 120, fps: Int32 = 24
let writer = try AVAssetWriter(outputURL: output, fileType: .mp4)
let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
    AVVideoCodecKey: AVVideoCodecType.h264,
    AVVideoWidthKey: width, AVVideoHeightKey: height,
    AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: 180_000]
])
let adapter = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input,
    sourcePixelBufferAttributes: [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32ARGB,
        kCVPixelBufferWidthKey as String: width,
        kCVPixelBufferHeightKey as String: height
    ])
guard writer.canAdd(input) else { fatalError("H.264 encoding unavailable") }
writer.add(input)
guard writer.startWriting() else { fatalError("Cannot start synthetic encoder") }
writer.startSession(atSourceTime: .zero)
let deadline = Date().addingTimeInterval(30)
for frame in 0..<frames {
    while !input.isReadyForMoreMediaData {
        guard Date() < deadline, writer.status == .writing else {
            fatalError("Timed out producing synthetic video")
        }
        Thread.sleep(forTimeInterval: 0.005)
    }
    try autoreleasepool {
        var buffer: CVPixelBuffer?
        guard let pool = adapter.pixelBufferPool,
              CVPixelBufferPoolCreatePixelBuffer(nil, pool, &buffer) == kCVReturnSuccess,
              let pixels = buffer else { fatalError("Cannot allocate video frame") }
        CVPixelBufferLockBaseAddress(pixels, [])
        let base = CVPixelBufferGetBaseAddress(pixels)!.assumingMemoryBound(to: UInt8.self)
        let stride = CVPixelBufferGetBytesPerRow(pixels)
        for y in 0..<height {
            for x in 0..<width {
                let i = y * stride + x * 4
                let bar = abs(x - frame * (width - 1) / (frames - 1)) < 8
                base[i] = 255
                base[i + 1] = bar ? 245 : (x < width / 3 ? 220 : 25)
                base[i + 2] = bar ? 245 : (x >= width / 3 && x < 2 * width / 3 ? 180 : 35)
                base[i + 3] = bar ? 245 : (x >= 2 * width / 3 ? 215 : 45)
            }
        }
        CVPixelBufferUnlockBaseAddress(pixels, [])
        guard adapter.append(pixels, withPresentationTime: CMTime(value: Int64(frame), timescale: fps)) else {
            throw writer.error ?? NSError(domain: "SyntheticVideo", code: 1)
        }
    }
}
input.markAsFinished()
let done = DispatchSemaphore(value: 0)
writer.finishWriting { done.signal() }
guard done.wait(timeout: .now() + 30) == .success, writer.status == .completed else {
    fatalError("Synthetic video did not finish")
}
print("Created silent 320x180 H.264: 120 frames, 24 fps, 5 seconds")
