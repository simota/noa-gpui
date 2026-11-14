#!/usr/bin/with-contenv bash
# Container-start hook (runs as root via /custom-cont-init.d). Idempotent.
set -euo pipefail

packages=(
  curl ca-certificates build-essential pkg-config
  libx11-dev libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev
  mesa-vulkan-drivers libvulkan1 vulkan-tools libgl1-mesa-dri
  fonts-noto-cjk locales fcitx5 fcitx5-mozc fcitx5-config-qt im-config
)

missing=()
for p in "${packages[@]}"; do
  dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p")
done
if ((${#missing[@]})); then
  # The image ships a NodeSource repository whose TLS check fails from this
  # container, which makes `apt-get update` fail. Nothing here needs Node.
  if [ -f /etc/apt/sources.list.d/nodesource.sources ]; then
    mv /etc/apt/sources.list.d/nodesource.sources /etc/apt/sources.list.d/nodesource.sources.disabled
  fi
  echo "[noa-setup] installing: ${missing[*]}"
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${missing[@]}"
fi

if ! locale -a | grep -qi '^ja_JP.utf8$'; then
  sed -i 's/^# *ja_JP.UTF-8/ja_JP.UTF-8/' /etc/locale.gen
  locale-gen
fi

cat >/etc/profile.d/noa-ime.sh <<'EOF'
export XMODIFIERS=@im=fcitx
export GTK_IM_MODULE=fcitx
export QT_IM_MODULE=fcitx
EOF
mkdir -p /etc/xdg/autostart
cp -f /usr/share/applications/org.fcitx.Fcitx5.desktop /etc/xdg/autostart/ 2>/dev/null || true

if [ ! -x /config/.cargo/bin/cargo ]; then
  echo "[noa-setup] installing rustup"
  s6-setuidgid abc bash -c \
    'curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable'
fi
echo "[noa-setup] done"
