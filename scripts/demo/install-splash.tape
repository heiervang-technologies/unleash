Output scripts/demo/install-splash.gif

Set Shell "bash"
Set FontSize 14
Set Width 1200
Set Height 800
Set Padding 20
Set Theme "Catppuccin Mocha"

Type "target/release/splash"
Enter
Sleep 2s

# Splash picker cycles through all 7 supported agents.
# Order (from src/bin/splash.rs): claude → codex → antigravity → opencode → pi → hermes → gemini → claude

# claude → codex (grey)
Right
Sleep 1.2s

# codex → antigravity (purple)
Right
Sleep 1.2s

# antigravity → opencode (green)
Right
Sleep 1.2s

# opencode → pi (purple)
Right
Sleep 1.2s

# pi → hermes (indigo)
Right
Sleep 1.2s

# hermes → gemini (blue)
Right
Sleep 1.2s

# gemini → claude (back to orange)
Right
Sleep 1.2s

# Confirm selection
Enter
Sleep 500ms
