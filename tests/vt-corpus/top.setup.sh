#!/bin/sh
# Setup for top.steps: launch.sh starts a busy perl process (it ends itself after 25 s) and runs
# the interactive macOS top on that one process. top then shows the system totals, which hold no
# personal data, and one process of ours. Without -pid, top would list the processes of the
# machine, and their names and users would be in the recording.
set -e

cat > launch.sh <<'EOF'
#!/bin/sh
perl -e 'alarm 25; 1 while 1' &
exec /usr/bin/top -o cpu -s 1 -pid $! \
    -stats pid,command,cpu,time,th,wq,ports,mem,purg,cmprs,pgrp,ppid,state,boosts,cpu_me,cpu_others,faults,cow
EOF
chmod +x launch.sh
