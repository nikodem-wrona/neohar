#!/bin/sh
# Setup for htop.steps: an htoprc without the USER column, and launch.sh, which starts three
# workload processes (each ends itself after 40 s) and runs htop on those three and on htop itself
# with -p. Without -p, htop would list the processes of the machine, with their users and command
# lines, and they would be in the recording.
#
# The htoprc has no htop_version line, so htop reads `fields` as numbers: 0 PID, 17 PRIORITY,
# 18 NICE, 38 M_VIRT, 39 M_RESIDENT, 2 STATE, 46 PERCENT_CPU, 47 PERCENT_MEM, 49 TIME, 1 Command.
set -e

mkdir -p "$HOME/.config/htop"
cat > "$HOME/.config/htop/htoprc" <<'EOF'
fields=0 17 18 38 39 2 46 47 49 1
hide_kernel_threads=1
hide_userland_threads=0
show_program_path=0
highlight_base_name=1
highlight_threads=1
tree_view=0
header_margin=1
detailed_cpu_time=0
cpu_count_from_one=0
show_cpu_usage=1
show_cpu_frequency=0
update_process_names=0
account_guest_in_cpu_meter=0
color_scheme=0
delay=10
left_meters=AllCPUs Memory Swap
left_meter_modes=1 1 1
right_meters=Tasks LoadAverage Uptime
right_meter_modes=2 2 2
EOF

cat > launch.sh <<'EOF'
#!/bin/sh
perl -e 'alarm 40; 1 while 1' &
a=$!
perl -e 'alarm 40; select(undef, undef, undef, 0.02) while 1' &
b=$!
sleep 40 &
c=$!
exec /opt/homebrew/bin/htop -p $$,$a,$b,$c
EOF
chmod +x launch.sh
