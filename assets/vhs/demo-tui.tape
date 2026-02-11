# VHS Demo for Agent Unleashed TUI
# Shows all TUI submenus and navigation
# https://github.com/charmbracelet/vhs

Output demo-tui.gif

Set FontSize 14
Set Width 1400
Set Height 900
Set Theme "Dracula"
Set Padding 20

# Launch the TUI
Type "aui"
Sleep 500ms
Enter
Sleep 2s

# Show main menu - navigate down through options
# The TUI starts on "Start Claude"
Sleep 500ms

# Navigate to "Update TUI" (down 1)
Down
Sleep 800ms

# Navigate to "Profiles" (down 2)
Down
Sleep 800ms

# Navigate to "Settings" (down 3)
Down
Sleep 800ms

# Navigate to "Manage Versions" (down 4)
Down
Sleep 800ms

# Navigate to "Help" (down 5)
Down
Sleep 800ms

# Navigate back up to "Start Claude"
Up
Sleep 400ms
Up
Sleep 400ms
Up
Sleep 400ms
Up
Sleep 400ms
Up
Sleep 800ms

# Navigate to "Update TUI" and enter
Down
Sleep 400ms
Enter
Sleep 2s

# Back to main menu
Escape
Sleep 800ms

# Navigate to "Profiles" and enter
Down
Sleep 400ms
Down
Sleep 400ms
Enter
Sleep 1500ms

# Show profiles list
Sleep 1s

# Back to main menu
Escape
Sleep 800ms

# Navigate to "Settings" and enter
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Enter
Sleep 1500ms

# Show settings - navigate through options
Down
Sleep 600ms
Down
Sleep 600ms
Down
Sleep 600ms
Up
Sleep 600ms
Up
Sleep 600ms

# Back to main menu
Escape
Sleep 800ms

# Navigate to "Manage Versions" and enter
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Enter
Sleep 1500ms

# Show versions list - scroll through
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Up
Sleep 400ms
Up
Sleep 400ms

Sleep 1s

# Back to main menu
Escape
Sleep 800ms

# Navigate to "Help" and enter
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Enter
Sleep 2s

# Back to main menu
Escape
Sleep 800ms

# Exit the TUI
Type "q"
Sleep 1s
