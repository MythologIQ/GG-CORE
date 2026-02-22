@echo off
setlocal

set LIBCLANG_PATH=C:\Program Files\llvm15.0.7\bin
set CMAKE_GENERATOR=Visual Studio 17 2022

cd /d G:\MythologIQ\CORE\core-runtime

echo Building and running E2E test...
cargo test --features gguf e2e_load_and_generate -- --nocapture

endlocal
