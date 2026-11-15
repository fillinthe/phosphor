# Phosphor game pad: kit for your 3D project

People at the show get a game pad on their phones (up, down, left, right, A, B) and take turns flying a camera
through your world. Phosphor runs the turns and sends every press to your project as OSC over the network.
Your project keeps everything it has: lighting, materials, sky, post-processing. The script only moves the camera.

## What Phosphor sends

OSC messages over UDP, to port **{PORT}** on the computer running your project:

| Address | Values | Meaning |
|---|---|---|
| `/pad/1/up`, `/down`, `/left`, `/right` | `1` or `0` | held or let go |
| `/pad/1/a` | `1` or `0` | **A = boost** (faster while held) |
| `/pad/1/b` | `1` or `0` | **B = trigger** (fires something in your world) |
| `/pad/1/stick` | `x y` (each -1, 0 or 1) | the four arrows as one direction |

`1` is the player number. One pad with turns always sends player 1.

## Unity

1. Copy `Unity/PhosphorPad.cs` into your project's `Assets` folder.
2. Add the **Phosphor Pad** component to your camera (or to any object that should fly).
3. Press Play. In the Inspector you can set the speed, turn rate, boost, smoothing, and a box to stay inside.
4. **B** calls the `On Trigger` event: hook up anything there (a particle burst, a door, a sound).

No plugin is needed: the script reads OSC itself.

## Unreal

Two ways. The Blueprint way needs no C++.

**Blueprint only (built-in OSC plugin):**
1. Edit › Plugins: turn on **OSC**. Restart the editor.
2. In your pawn's or camera actor's Blueprint, on Begin Play: **Create OSC Server** (address `0.0.0.0`, port {PORT},
   Start Listening on), and keep it in a variable.
3. From the server: **Bind Event to On OSC Message Received**. In the event, **Get OSC Message Address** and
   **Get OSC Message Floats**; compare the address with `/pad/1/stick`, `/pad/1/a`, `/pad/1/b`.
4. On Tick: move the actor forward by `stick.y * speed * delta`, turn it by `stick.x * turn rate * delta`
   (double the speed while A is held). On B going to 1: do your trigger.

**C++ component:** copy `Unreal/PhosphorPadComponent.h` and `.cpp` into your project's `Source/<YourProject>/`
folder, replace `YOURPROJECT_API` with your module's API macro, add `"Networking", "Sockets"` to
`PublicDependencyModuleNames` in your `.Build.cs`, build, then add **Phosphor Pad** to your pawn or camera actor.
B fires its `On Trigger` event (bind it in Blueprint).

## Sending your picture back to Phosphor

- **Unreal:** the NDI plugin (NDI Broadcast Actor on your camera), or Spout on Windows.
- **Unity:** KlakNDI (NDI Sender on your camera), or KlakSpout on Windows.

In Phosphor it shows up under **Inputs**: drag it onto a layer.

## Testing without a crowd

In Phosphor: Session › Audience play › Game pad on, "Send pad to" your computer's address and port {PORT}. Open
the remote page on any phone or browser (Session shows the link) and press the pad: the remote counts as player 1.

Both computers must be on the same network, and the firewall must let UDP port {PORT} in.
