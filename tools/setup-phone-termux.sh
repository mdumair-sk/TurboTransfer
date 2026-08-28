#!/data/data/com.termux/files/usr/bin/bash
# ==============================================================================
# TurboTransfer - Snapdragon 8 Elite Native Build Node Setup
# ==============================================================================
# Run this script inside Termux on your Android phone to set up:
# - Rust toolchain (cargo, rustc native aarch64)
# - Clang / LLVM C compiler and lld linker
# - OpenSSH daemon for high-speed ADB tunnel communication
# - Build directories and environment optimizations
# ==============================================================================

set -e

echo "🚀 [1/5] Updating Termux package repositories..."
pkg update -y && pkg upgrade -y

echo "📦 [2/5] Installing Rust, Clang, OpenSSH, and build utilities..."
pkg install -y rust clang binutils git openssh tar make pkg-config

# Optional performance enhancements
echo "⚡ [3/5] Installing build speed utilities..."
pkg install -y sccache || true

echo "🔑 [4/5] Setting up SSH server..."
mkdir -p ~/.ssh
chmod 700 ~/.ssh
touch ~/.ssh/authorized_keys
chmod 600 ~/.ssh/authorized_keys

# If a public key was pushed to home, import it
if [ -f /data/data/com.termux/files/home/id_rsa.pub ]; then
    cat /data/data/com.termux/files/home/id_rsa.pub >> ~/.ssh/authorized_keys
    rm -f /data/data/com.termux/files/home/id_rsa.pub
    echo "  -> Imported public key to ~/.ssh/authorized_keys"
fi

# Generate host keys if not present
ssh-keygen -A

# Start SSH daemon on standard Termux port 8022
pkill sshd || true
sshd

echo "📁 [5/5] Creating TurboTransfer project build cache directory..."
mkdir -p ~/turbotransfer
mkdir -p ~/.cargo

# Optimize Cargo for Snapdragon 8 Elite (8 Oryon cores)
cat << 'EOF' > ~/.cargo/config.toml
[build]
# Utilize all 8 Oryon performance/prime cores
jobs = 8

[target.aarch64-linux-android]
linker = "clang"
rustflags = ["-C", "target-cpu=native", "-C", "link-arg=-fuse-ld=lld"]

[target.aarch64-unknown-linux-android]
linker = "clang"
rustflags = ["-C", "target-cpu=native", "-C", "link-arg=-fuse-ld=lld"]
EOF

# Ensure sshd auto-starts on Termux launch
if ! grep -q "sshd" ~/.bashrc 2>/dev/null; then
    echo "sshd" >> ~/.bashrc
fi

echo ""
echo "================================================================="
echo "🎉 Snapdragon 8 Elite Build Node is READY!"
echo "   - Native Rust: $(rustc --version)"
echo "   - Clang: $(clang --version | head -n 1)"
echo "   - SSH Daemon: Running on port 8022"
echo "   - Working directory: ~/turbotransfer"
echo "================================================================="
