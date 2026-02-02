# actix-ws-redis-protobuf

Actix Web WebSocket broadcast hub with optional Protobuf and Redis integration.

## Run the example server

- JSON → WS text broadcast:
  - `cargo run --bin server`
  - `python3 client.py`
- Enable Protobuf endpoints:
  - `cargo run --features protobuf --bin server`
  - `python3 client_proto.py`
- Enable Redis subscribe/publish (binary payloads on a channel):
  - `REDIS_URL=redis://127.0.0.1/ REDIS_CHANNEL=events cargo run --features redis --bin server`

## Endpoints (server)

- `GET /ws/` WebSocket endpoint (broadcasts incoming client text/binary to all connected clients)
- `POST /ws/json/json/stuff` accepts JSON and broadcasts JSON as WS text
- `POST /ws/json/pb/stuff` (feature `protobuf`) accepts JSON and broadcasts protobuf as WS binary
- `POST /ws/pb/pb/stuff` (feature `protobuf`) accepts protobuf and broadcasts protobuf as WS binary
