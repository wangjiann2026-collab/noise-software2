#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# NoiseCad macOS 前置环境安装脚本
# 在 Mac 上运行一次：bash scripts/setup-mac.sh
# ─────────────────────────────────────────────────────────────────────────────
set -e

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
ok()   { echo -e "${GREEN}✓${NC} $1"; }
warn() { echo -e "${YELLOW}⚠${NC}  $1"; }
step() { echo -e "\n${YELLOW}▶ $1${NC}"; }

echo -e "\n══════════════════════════════════════════"
echo   "   NoiseCad Desktop — macOS 环境安装"
echo   "══════════════════════════════════════════"

# ── 1. Xcode Command Line Tools ──────────────────────────────────────────────
step "检查 Xcode Command Line Tools"
if xcode-select -p &>/dev/null; then
    ok "Xcode CLT 已安装 ($(xcode-select -p))"
else
    warn "未检测到 Xcode CLT，正在安装..."
    xcode-select --install
    echo "  请在弹出对话框中点击「安装」，完成后重新运行本脚本"
    exit 0
fi

# ── 2. Homebrew ──────────────────────────────────────────────────────────────
step "检查 Homebrew"
if command -v brew &>/dev/null; then
    ok "Homebrew $(brew --version | head -1)"
else
    warn "未安装 Homebrew，正在安装..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    # Apple Silicon path
    eval "$(/opt/homebrew/bin/brew shellenv)" 2>/dev/null || true
fi

# ── 3. Rust ──────────────────────────────────────────────────────────────────
step "检查 Rust"
if command -v rustup &>/dev/null; then
    ok "Rust $(rustc --version)"
    rustup update stable --no-self-update
else
    warn "未安装 Rust，正在安装..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    source "$HOME/.cargo/env"
fi

# ── 4. Rust targets for universal binary ────────────────────────────────────
step "添加 Rust 交叉编译 targets"
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
ok "aarch64-apple-darwin  x86_64-apple-darwin 已添加"

# ── 5. tauri-cli ────────────────────────────────────────────────────────────
step "检查 cargo-tauri"
if cargo tauri --version &>/dev/null; then
    ok "tauri-cli $(cargo tauri --version)"
else
    warn "安装 tauri-cli..."
    cargo install tauri-cli --version '^2' --locked
fi

# ── 6. 生成图标 ──────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ICONS_DIR="$SCRIPT_DIR/apps/noise-desktop/src-tauri/icons"

step "生成应用图标"
if [ -f "$ICONS_DIR/icon-source.png" ]; then
    cd "$SCRIPT_DIR/apps/noise-desktop"
    cargo tauri icon src-tauri/icons/icon-source.png
    ok "图标已生成 ($ICONS_DIR)"
else
    warn "找不到 icon-source.png，跳过图标生成"
    warn "请将 1024×1024 PNG 放置于 $ICONS_DIR/icon-source.png 后运行 make icons"
fi

# ── 完成 ─────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}══════════════════════════════════════════${NC}"
echo -e "${GREEN}   环境安装完成！${NC}"
echo -e "${GREEN}══════════════════════════════════════════${NC}"
echo ""
echo "  下一步："
echo ""
echo "  cd apps/noise-desktop"
echo ""
echo "  make dev              # 开发模式（热重载）"
echo "  make build            # 发布构建（当前架构）"
echo "  make build-universal  # 通用二进制（arm64 + x86_64）"
echo "  make dmg              # 构建 + 生成 DMG 安装包"
echo ""
