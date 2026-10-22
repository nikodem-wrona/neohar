#!/bin/sh
# Setup for less.steps: big.txt, 3,000 lines in the working directory. Some lines have colours
# (SGR with 16 colours, 256 colours, RGB, bold and underline), some have wide characters, an
# emoji and a combining mark, some are longer than the screen is wide, and every 400th line has
# a search marker NEEDLE-nnnn.
set -e

# "e" and U+0301 (combining acute accent) as UTF-8 bytes.
combining=$(printf 'e\314\201')

awk -v combining="$combining" 'BEGIN {
    esc = sprintf("%c", 27)
    for (i = 1; i <= 3000; i++) {
        line = sprintf("Line %04d: the quick brown fox jumps over the lazy dog", i)
        if (i % 7 == 0)
            line = line " " esc "[31mred" esc "[0m and " esc "[1;32mbold green" esc "[0m"
        if (i % 13 == 0)
            line = line " " esc "[38;5;208morange" esc "[0m " esc "[4munderlined" esc "[0m"
        if (i % 17 == 0)
            line = line " " esc "[38;2;255;100;0m" esc "[48;2;10;40;90mrgb colours" esc "[0m"
        if (i % 11 == 0)
            line = line " 日本語のテキスト 漢字 ｗｉｄｅ ｔｅｘｔ"
        if (i % 50 == 0)
            line = line " 🎉 party, caf" combining " au lait"
        if (i % 97 == 0)
            line = line " lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua"
        if (i % 400 == 0)
            line = line sprintf(" NEEDLE-%04d", i / 400)
        print line
    }
}' > big.txt
