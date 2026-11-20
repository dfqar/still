#!/bin/zsh
set -euo pipefail

repo_root="${0:A:h:h}"
app_binary="$repo_root/build/DerivedData/Build/Products/Release/Still.app/Contents/MacOS/Still"
duration_seconds="${1:-3600}"
sample_interval_seconds="${2:-10}"
output_dir="$repo_root/build/perf"
profile_name="${STILL_SOAK_PROFILE:-all-scenes}"
samples_path="$output_dir/soak-$profile_name-memory.tsv"
metrics_path="$output_dir/soak-$profile_name-metrics.json"
stderr_path="$output_dir/soak-$profile_name-stderr.log"

if [[ ! -x "$app_binary" ]]; then
    print -u2 "Release app is missing. Run: cargo xtask package"
    exit 1
fi

if ! [[ "$duration_seconds" == <-> && "$sample_interval_seconds" == <-> ]] || (( duration_seconds == 0 || sample_interval_seconds == 0 )); then
    print -u2 "Usage: $0 [duration-seconds] [sample-interval-seconds]"
    exit 2
fi

if ! [[ "$profile_name" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    print -u2 "Profile name must contain only letters, digits, underscore or hyphen"
    exit 2
fi

mkdir -p "$output_dir"
print $'elapsed_seconds\trss_kib\tcpu_percent' > "$samples_path"
: > "$stderr_path"

STILL_PROFILE_WHEN_INACTIVE=1 \
STILL_PROFILE_RUN=1 \
STILL_QUALITY="${STILL_SOAK_QUALITY:-2}" \
STILL_EXIT_AFTER_SECONDS="$duration_seconds" \
STILL_METRICS_PATH="$metrics_path" \
STILL_INITIAL_SCENE="${STILL_SOAK_INITIAL_SCENE:-drift}" \
STILL_SCENE_CYCLE_SECONDS="${STILL_SOAK_CYCLE_SECONDS:-60}" \
    "$app_binary" 2> "$stderr_path" &
app_pid=$!
start_seconds=$(date +%s)

while kill -0 "$app_pid" 2>/dev/null; do
    now_seconds=$(date +%s)
    elapsed_seconds=$((now_seconds - start_seconds))
    if (( elapsed_seconds > duration_seconds )); then
        break
    fi

    process_sample=$(ps -o rss=,%cpu= -p "$app_pid" | xargs)
    if [[ -n "$process_sample" ]]; then
        rss_kib=${process_sample%% *}
        cpu_percent=${process_sample##* }
        print "$elapsed_seconds\t$rss_kib\t$cpu_percent" >> "$samples_path"
    fi
    sleep "$sample_interval_seconds"
done

wait "$app_pid"
print "Soak samples: $samples_path"
print "Frame metrics: $metrics_path"
print "Standard error: $stderr_path"
