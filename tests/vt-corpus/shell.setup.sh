#!/bin/sh
# Setup for shell.steps: files with names that make Tab completion deterministic, and a command
# with a unique name in {work}/bin (the recording puts that directory first in PATH).
#   quarterly-report-final.txt   the only file that starts with "quar"
#   notes-a.txt, notes-b.txt     two files that start with "notes-" (an ambiguous prefix)
#   zzprobe-greet                the only command that starts with "zzprobe"
set -e

echo 'Q3 report: all numbers are made up' > quarterly-report-final.txt
echo 'first note' > notes-a.txt
echo 'second note' > notes-b.txt

mkdir -p bin
cat > bin/zzprobe-greet <<'EOF'
#!/bin/sh
echo "hello from zzprobe-greet"
EOF
chmod +x bin/zzprobe-greet
