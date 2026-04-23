@echo off
setlocal
cargo build --release --target x86_64-pc-windows-msvc
if errorlevel 1 (
  echo Build failed.
  exit /b 1
)
echo.
echo Build complete:
echo target\x86_64-pc-windows-msvc\release\imclicker_v2.exe
