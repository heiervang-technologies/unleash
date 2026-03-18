# VHS Demo for Agent Unleashed
# https://github.com/charmbracelet/vhs

Output demo.gif

Set FontSize 16
Set Width 1200
Set Height 800
Set Theme "Dracula"
Set Padding 20

# Show the main help command
Type "au"
Sleep 500ms
Enter
Sleep 3s

# Show version management
Type "au version"
Sleep 500ms
Enter
Sleep 2s

# Show available versions
Type "au version --list"
Sleep 500ms
Enter
Sleep 3s

# Show authentication status
Type "au auth"
Sleep 500ms
Enter
Sleep 2s

# Launch the TUI (exit quickly with q)
Type "aui"
Sleep 500ms
Enter
Sleep 3s
Type "q"
Sleep 1s

# Show headless mode status
Type "autx status"
Sleep 500ms
Enter
Sleep 2s

# Start Claude with wrapper features (show command, don't actually run)
Type "# Start Claude with wrapper features:"
Sleep 500ms
Enter
Sleep 500ms
Type "aug"
Sleep 1s
Ctrl+C
Sleep 500ms

# Show auto mode option
Type "# Or in autonomous mode:"
Sleep 500ms
Enter
Sleep 500ms
Type "aug --auto"
Sleep 2s

Sleep 2s
