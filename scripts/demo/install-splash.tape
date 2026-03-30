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

# Cycle to codex (grey)
Right
Sleep 1.5s

# Cycle to gemini (purple)
Right
Sleep 1.5s

# Cycle to opencode (blue)
Right
Sleep 1.5s

# Cycle back to claude (orange)
Right
Sleep 1.5s

# Confirm selection
Enter
Sleep 500ms
