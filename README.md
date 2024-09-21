# FRCLib NetTables

A server and client implementation of the [NT4 Spec](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc) for Rust.
The client is forked from [Tsar Boomba's Implementation](https://github.com/tsar-boomba/network-tables-rs)

## Goals
- [X] Rust Client connect to NTCore Server
- [X] NTCore Client connect to Rust Server
- [X] Rust Client connect to Rust Server
- [X] Metatopic compliant
- [ ] Document protocol better (happening in `rewrite` branch)
- [ ] Allow running on any async executor/runtime (happening in `rewrite` branch)
- [ ] Better support for publisher config (not caching and persistent topics) (happening in `rewrite` branch)
- [ ] RTT websocket of NT4.1  (happening in `rewrite` branch)
