#!/bin/bash

# Cross-compile for Windows
echo "Building for Windows x86_64..."

# Build for Windows
cargo build --release --target x86_64-pc-windows-gnu

if [ $? -eq 0 ]; then
    echo "✅ Windows build successful!"
    echo "📁 Binary location: target/x86_64-pc-windows-gnu/release/nwn_parser.exe"
    
    # Show file size
    ls -lh target/x86_64-pc-windows-gnu/release/nwn_parser.exe
else
    echo "❌ Windows build failed!"
    exit 1
fi