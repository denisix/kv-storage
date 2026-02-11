#!/bin/bash
# Simple benchmark summary for KV Storage
# Shows only operations per second (op/s) for easy performance comparison

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}  KV Storage Benchmark Results${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

# Run benchmarks and capture output
echo -e "${YELLOW}Running benchmarks...${NC}"
cargo bench --bench kv_bench 2>&1 > .bench_output.txt

echo ""
echo -e "${GREEN}=== Operations Per Second (op/s) ===${NC}"
echo ""

# Function to extract throughput value - handles both Melem/s and Kelem/s
extract_thrpt() {
    grep -a -A 1 "$1" .bench_output.txt | grep -a "thrpt:" | tail -1 | sed -E 's/.*thrpt:.*\[([0-9.]+)\s*([KMGT]?)elem\/s\].*/\1 \2/' | head -1
}

# Function to format number with K/M suffix
format_ops() {
    local value=$1
    local unit=$2
    local num=$(echo "$value" | sed 's/[^0-9.]//g')

    case "$unit" in
        M)
            # Convert to millions with 2 decimal places
            echo "$(printf "%.2f" $(echo "$num * 1000000" | bc 2>/dev/null || echo "$num"))M"
            ;;
        K)
            # Convert to thousands with 0 decimal places
            echo "$(printf "%.0f" $(echo "$num * 1000" | bc 2>/dev/null || echo "$num"))K"
            ;;
        "")
            # Plain number with commas
            echo "$num" | sed ':a;s/\B[0-9]\{3\}\>/,/g;ta'
            ;;
        *)
            echo "$num"
            ;;
    esac
}

# Write Throughput
echo "Write Throughput:"
for size in 100 1000; do
    result=$(extract_thrpt "write_throughput/$size")
    if [ -n "$result" ]; then
        value=$(echo "$result" | awk '{print $1}')
        unit=$(echo "$result" | awk '{print $2}')
        formatted=$(format_ops "$value" "$unit")
        printf "  %-12s: ${GREEN}%s${NC} op/s\n" "$size items" "$formatted"
    fi
done
echo ""

# Read Throughput
echo "Read Throughput:"
for size in 100 1000 10000; do
    result=$(extract_thrpt "read_throughput/$size")
    if [ -n "$result" ]; then
        value=$(echo "$result" | awk '{print $1}')
        unit=$(echo "$result" | awk '{print $2}')
        formatted=$(format_ops "$value" "$unit")
        printf "  %-12s: ${GREEN}%s${NC} op/s\n" "$size items" "$formatted"
    fi
done
echo ""

# Hash Performance (in ops/sec - convert from time)
echo "Hash Performance (xxHash3-128):"
for size in 1k 4k 16k; do
    # Extract the time value from the time line
    time_line=$(grep -a -A 2 "hash/xxhash3_128_$size" .bench_output.txt | grep -a "time:" | tail -1)
    time_ns=$(echo "$time_line" | sed -E 's/.*time:\s+\[[^]]*\s+([0-9.]+)\s+ns\].*/\1/')
    if [ -n "$time_ns" ] && [[ "$time_ns" =~ ^[0-9.]+$ ]]; then
        # Convert ns/op to ops/sec (1e9 / ns_per_op)
        ops_sec=$(echo "scale=0; 1000000000 / $time_ns" | bc 2>/dev/null)
        formatted=$(format_ops "$ops_sec" "")
        case "$size" in
            1k) label="1 KB" ;;
            4k) label="4 KB" ;;
            16k) label="16 KB" ;;
        esac
        printf "  %-12s: ${GREEN}%s${NC} op/s\n" "$label" "$formatted"
    fi
done
echo ""

# Compression Performance
echo "Compression Performance:"
compress_line=$(grep -a -A 2 "compression/compress_4k_repeatable" .bench_output.txt | grep -a "time:" | tail -1)
compress_us=$(echo "$compress_line" | sed -E 's/.*time:\s+\[[^]]*\s+([0-9.]+)\s+µs\].*/\1/')
if [ -n "$compress_us" ] && [[ "$compress_us" =~ ^[0-9.]+$ ]]; then
    ops_sec=$(echo "scale=0; 1000000 / $compress_us" | bc 2>/dev/null)
    formatted=$(format_ops "$ops_sec" "")
    printf "  %-12s: ${GREEN}%s${NC} op/s\n" "Compress" "$formatted"
fi

decompress_line=$(grep -a -A 2 "compression/decompress_4k" .bench_output.txt | grep -a "time:" | tail -1)
decompress_us=$(echo "$decompress_line" | sed -E 's/.*time:\s+\[[^]]*\s+([0-9.]+)\s+µs\].*/\1/')
if [ -n "$decompress_us" ] && [[ "$decompress_us" =~ ^[0-9.]+$ ]]; then
    ops_sec=$(echo "scale=0; 1000000 / $decompress_us" | bc 2>/dev/null)
    formatted=$(format_ops "$ops_sec" "")
    printf "  %-12s: ${GREEN}%s${NC} op/s\n" "Decompress" "$formatted"
fi
echo ""

echo -e "${BLUE}======================================${NC}"
echo "Full report: target/criterion/report/index.html"
echo -e "${BLUE}======================================${NC}"

# Cleanup
rm -f .bench_output.txt
