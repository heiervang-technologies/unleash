# VHS Demo: Claude Slide Animation
# Watch Claude slide across the screen when navigating between menus!
# The full figure appears during transition, then clips at the sidebar edge.

Output demo-animation.gif

Require cui

Set FontSize 14
Set Width 1400
Set Height 800
Set Framerate 60

# Start the TUI
Type "cui"
Enter
Sleep 1500ms

# Show the main menu - Claude is on the left side
Sleep 500ms

# Navigate to Profiles - Claude slides from left to right!
Down
Sleep 100ms
Enter
Sleep 800ms

# Pause to admire the result
Sleep 800ms

# Go back - Claude slides back from right to left!
Escape
Sleep 800ms
Sleep 500ms

# Do it again to show the smooth animation
Down
Sleep 100ms
Enter
Sleep 800ms
Sleep 500ms

# Go back
Escape
Sleep 800ms
Sleep 500ms

# Navigate to Claude Code Version
Down
Sleep 100ms
Enter
Sleep 800ms
Sleep 500ms

# Go back
Escape
Sleep 800ms
Sleep 500ms

# Quick demo - rapid navigation showing continuous sliding
Down
Enter
Sleep 800ms
Escape
Sleep 800ms
Down
Enter
Sleep 800ms
Escape
Sleep 800ms

# Final pause
Sleep 1s

# Quit
Type "q"
Sleep 500ms
