# Syncbus

Syncbus is a D-Bus server for [Syncthing](https://syncthing.net/). It exposes
few functionalities of Syncthing through D-Bus properties and methods.

This server is based on the v2.0.0 of Syncthing [REST API](https://docs.syncthing.net/v2.0.0/dev/rest.html).

## Prerequisites

Syncbus requires a few runtime dependencies.

1. Syncthing must be configured to be available via HTTP API.
2. Systemd with a `syncthing.service` unit to manage Syncthing.

## Getting Started

Syncbus is written in Rust, so it needs standard Rust development setup.

```sh
$ cargo run
```

You can use
[`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) to
configure logging. For example, to enable debug logging, use `RUST_LOG=debug`.

```sh
$ RUST_LOG=debug cargo run
```

## Demo

Syncbus comes with a simple demo written in
[Adwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/). It is meant to
demonstrate the different APIs of the server.

![](./data/screenshots/demo-1.png)

The demo can be built by using the `demo` package name.

As the demo communicates with the D-Bus server, you need to have the server
running before the demo is launched.

```sh
$ cargo run&
$ cargo run -p demo
```

## API

Please check [`docs/api.md`](./docs/api.md).

## Getting in Touch

Please use [`phosh.mobi`](https://matrix.to/#/#phosh:phosh.mobi) Matrix channel
to communicate with the developers.
