# VHS Demo for Theme Customization
# Showcases the new Theme menu with 7 color presets
# https://github.com/charmbracelet/vhs

Require aui

Output assets/demo-theme.gif

Set FontSize 14
Set Width 1400
Set Height 900
Set Theme "Dracula"
Set Padding 20
Set Framerate 30

# Launch the TUI
Type "aui"
Sleep 500ms
Enter
Sleep 2s

# Main menu starts at "Start Session" with orange mascot
Sleep 1s

# Navigate to Theme (index 4: Down x4)
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 600ms

# Enter Theme screen
Enter
Sleep 1500ms

# Currently on Orange (the default, marked with *)
# Browse down through all presets to show the swatches
Down
Sleep 800ms

# Blue
Down
Sleep 800ms

# Green
Down
Sleep 800ms

# Purple
Down
Sleep 800ms

# Red
Down
Sleep 800ms

# Cyan
Down
Sleep 800ms

# Pink - at the bottom
Sleep 1200ms

# Go back up to Blue (5x Up from Pink)
Up
Sleep 400ms
Up
Sleep 400ms
Up
Sleep 400ms
Up
Sleep 400ms
Up
Sleep 600ms

# Select Blue - this applies the theme and returns to main
Enter
Sleep 2s

# Main menu now shows Blue mascot and blue accents
Sleep 1500ms

# Go back to Theme (Down x4)
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

# Blue is pre-selected. Go to Purple (Down x2)
Down
Sleep 500ms
Down
Sleep 600ms

# Select Purple
Enter
Sleep 2s

# Main menu with purple mascot and accents
Sleep 1500ms

# Go back to Theme (Down x4)
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

# Purple is pre-selected. Go to Cyan (Down x2)
Down
Sleep 500ms
Down
Sleep 600ms

# Select Cyan
Enter
Sleep 2s

# Main menu with cyan mascot and accents
Sleep 1500ms

# Go back to Theme (Down x4)
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

# Cyan is pre-selected. Go to Pink (Down x1)
Down
Sleep 600ms

# Select Pink
Enter
Sleep 2s

# Main menu with pink mascot and accents
Sleep 1500ms

# Restore Orange - go to Theme
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

# Pink is pre-selected (index 6). Go to Orange (Up x6)
Up
Sleep 300ms
Up
Sleep 300ms
Up
Sleep 300ms
Up
Sleep 300ms
Up
Sleep 300ms
Up
Sleep 500ms

# Select Orange to restore default
Enter
Sleep 2s

# Back to main with original orange
Sleep 1s

# Exit
Type "q"
Sleep 1s
