Set Shell "bash"
Set Width 1600
Set Height 900
Set FontSize 18
Set TypingSpeed 15ms
Output terminal-workbench.webp

Type "./launch-casefile.sh"
Enter
Wait+Screen@20s /CASEFILE/
Sleep 1s
Type "5"
Wait+Screen@10s /STRATEGIES/
Sleep 2s
Type "6"
Wait+Screen@10s /BOARDS/
Sleep 3s
Screenshot terminal-workbench.png
Type "q"
Sleep 500ms
