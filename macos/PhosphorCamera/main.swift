// Phosphor's virtual camera: a Core Media I/O camera extension. Apps (Zoom, OBS, Teams…) see a camera called
// "Phosphor"; Phosphor sends its picture into the device's sink stream and every frame goes straight out of the
// source stream. Until Phosphor sends anything, the camera shows black.
//
// macOS only loads it from inside Phosphor.app (Contents/Library/SystemExtensions), signed with a Developer ID and a
// provisioning profile that allows system extensions. See scripts/build-camera-extension.sh.

import CoreMediaIO
import Foundation

let provider = PhosphorProviderSource(clientQueue: nil)
CMIOExtensionProvider.startService(provider: provider.provider)
CFRunLoopRun()
