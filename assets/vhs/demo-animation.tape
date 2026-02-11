# VHS Demo: Claude Slide Animation
# Watch Claude smoothly slide across the screen when navigating menus!

Output demo-animation.gif

Require aui

Set FontSize 14
Set Width 1200
Set Height 700
Set Framerate 60

# Start the TUI
Type "aui"
Enter
Sleep 1s

# Main menu - Claude is on the right side
Sleep 300ms

# Navigate to Profiles - Claude slides left!
Down
Sleep 50ms
Enter
Sleep 700ms
Sleep 300ms

# Go back - Claude slides right!
Escape
Sleep 700ms
Sleep 300ms

# Navigate to Settings - Claude slides left again!
Down
Down
Sleep 50ms
Enter
Sleep 700ms
Sleep 300ms

# Go back - Claude slides right!
Escape
Sleep 700ms
Sleep 300ms

# Navigate to Help
Down
Down
Down
Sleep 50ms
Enter
Sleep 700ms
Sleep 300ms

# Go back
Escape
Sleep 700ms
Sleep 300ms

# Quick navigation demo
Down
Enter
Sleep 700ms
Escape
Sleep 700ms

# Final pause
Sleep 500ms

# Quit
Type "q"
Sleep 300ms
