# Linux desktop test environment

The Ubuntu + XFCE image of [linuxserver/webtop](https://docs.linuxserver.io/images/docker-webtop/) is used to check the demo on Linux (X11). You access the desktop from a browser.

The GPU is not passed into the container, so rendering uses Mesa's software drivers (lavapipe for Vulkan, llvmpipe for GL). It works for checking appearance and behavior, but not for measuring performance.

## Start

```sh
docker compose -f tools/linux-desktop/docker-compose.yml up -d
```

Open `http://127.0.0.1:3000/` to see the desktop.

Each time the container starts, `setup.sh` runs and installs only what is not yet installed: the packages needed for the build, the Vulkan software driver, Japanese fonts, fcitx5 + mozc, and rustup for the user `abc`. The first run takes a few minutes. Check `[noa-setup] done` in `docker logs noa-webtop` to see whether it has finished.

The repository is mounted read-only at `/config/noa-gpui`. Build output goes to `/config/noa-target` on a volume, so the host's `target/` is not changed.

## Build and run

```sh
docker exec -u abc -e HOME=/config -w /config/noa-gpui noa-webtop \
  bash -lc 'source ~/.cargo/env && cargo build -p desktop --release --locked'

docker exec -d -u abc -e HOME=/config -e DISPLAY=:1 -e XDG_RUNTIME_DIR=/config/.XDG \
  -e XMODIFIERS=@im=fcitx -e RUST_LOG=info noa-webtop \
  sh -c '/config/noa-target/release/noa-demo --scene=catalog > /config/noa-demo.log 2>&1'
```

Logs go to `/config/noa-demo.log`. You can also start `noa-demo` directly from a terminal inside the desktop.

## Stop and clean up

```sh
docker compose -f tools/linux-desktop/docker-compose.yml stop       # Stop (the container is kept)
docker compose -f tools/linux-desktop/docker-compose.yml start      # Resume
docker compose -f tools/linux-desktop/docker-compose.yml down       # Remove the container (volumes are kept)
docker compose -f tools/linux-desktop/docker-compose.yml down -v    # Also remove the volumes (rustup, build output)
```

- `stop` keeps the container, so packages installed with apt stay as well. Resume with `start` and it is ready right away. A container stopped by hand does not resume automatically when Docker restarts.
- `down` removes the container, so packages installed with apt are removed too. rustup and the build output on the volume remain. The next `up -d` takes a few minutes because `setup.sh` reinstalls the packages.
