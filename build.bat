@echo off
echo ==============================================
echo   Building LinkDeck (Release with Embedded Icon)
echo ==============================================
cargo build --release
if %errorlevel% neq 0 (
    echo [ERROR] Cargo build failed!
    exit /b %errorlevel%
)
python scripts\embed_icon.py
echo.
echo [SUCCESS] LinkDeck release binary is ready at: target\release\link-deck.exe
