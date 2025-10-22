# ScreenComposer ScreenCast portal

`screencomposer-portal` is a minimal implementation of the
`org.freedesktop.portal.ScreenCast` D-Bus interface tailored for ScreenComposer.
It exposes the well-known portal service
`org.freedesktop.portal.Desktop` on the session bus and provides stub
responses that allow PipeWire-capable clients (Chromium, OBS Studio, etc.) to
exercise the standard screencast flow during development or testing.

The implementation runs entirely in-process using [`zbus`](https://docs.rs/zbus)
for the D-Bus bindings and logs every method invocation and emitted signal via
`tracing`.

## Running

```bash
cargo run -p screencomposer-portal
```

On startup the binary prints `ScreenComposer portal running` once it has
successfully registered on the session bus. The process continues to service
requests until it receives `Ctrl+C`.

## Quick test

You can verify that the portal responds using `gdbus`:

```
gdbus call --session \
  --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop \
  --method org.freedesktop.portal.ScreenCast.CreateSession \
  "{'handle_token':<'t1'>,'session_handle_token':<'s1'>}"
```

The command returns immediately because the portal emits the corresponding
`org.freedesktop.portal.Request::Response` signal with a success payload as soon
as the method is invoked.

## TODO

This helper currently returns placeholder stream metadata and a dummy
PipeWire file descriptor. Integrating with real PipeWire session management and
ScreenComposer's renderer is left for future work.
