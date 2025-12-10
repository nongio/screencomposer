# PipeWire Video Source Example

Minimal PipeWire video source in DRIVER mode with self-allocated buffers (memfd).

## Run

```bash
cargo run
```

## View output

```bash
gst-launch-1.0 pipewiresrc path=<node_id> ! videoconvert ! autovideosink
```

The node ID is printed when the stream starts.

## Key concepts

- `DRIVER` mode: the source controls timing via a timer that calls `pw_stream_trigger_process()`
- `ALLOC_BUFFERS`: we allocate memfd buffers in `add_buffer` callback
- Buffers are tracked by fd and looked up in `process` to render frames
