#!/bin/bash

# Get the script's directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Set directories relative to script location
PROJECTS_DIR="${SCRIPT_DIR}/../projects"

# Create timestamped output directory
TIMESTAMP=$(date '+%Y-%m-%d-%H-%M-%S')
OUTPUT_DIR="${SCRIPT_DIR}/../results/${TIMESTAMP}_result"
RESULTS_FILE="${OUTPUT_DIR}/build_summary.txt"
ERRORS_FILE="${OUTPUT_DIR}/errors.txt"
SUCCESS_FILE="${OUTPUT_DIR}/success.txt"
TEMP_DIR="${OUTPUT_DIR}/temp"

# Number of parallel jobs
PARALLEL_JOBS=16

# Create output directories if they don't exist
mkdir -p "$OUTPUT_DIR"
mkdir -p "$TEMP_DIR"

# Clear previous results
> "$RESULTS_FILE"
> "$ERRORS_FILE"
> "$SUCCESS_FILE"

# Header
echo "========================================" | tee -a "$RESULTS_FILE"
echo "Sui Move Build Results (Parallel - ${PARALLEL_JOBS} threads)" | tee -a "$RESULTS_FILE"
echo "Started at: $(date)" | tee -a "$RESULTS_FILE"
echo "========================================" | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"

# Function to build a project
build_project() {
    local move_toml_path="$1"
    local project_dir="$(dirname "$move_toml_path")"
    local project_name="${project_dir#$PROJECTS_DIR/}"  # Get relative path from projects dir
    # Create a hash of the path for unique but shorter filename
    local hash=$(echo -n "$project_name" | md5sum | cut -c1-8)
    local short_name=$(basename "$project_dir")
    
    # Create individual log file for this project using hash and short name
    local log_file="${OUTPUT_DIR}/${short_name}_${hash}_build.log"
    
    # Create a temp result file for this specific build
    local temp_result="${TEMP_DIR}/${hash}_result.txt"
    
    echo "[$(date '+%H:%M:%S')] Building: $project_dir"
    
    # Change to project directory and run build
    cd "$project_dir" 2>/dev/null
    
    if [ $? -ne 0 ]; then
        echo "ERROR|Cannot access directory|$project_dir" > "$temp_result"
        return
    fi
    
    # Run sui move build and capture output
    sui move build > "$log_file" 2>&1
    BUILD_EXIT_CODE=$?
    
    if [ $BUILD_EXIT_CODE -eq 0 ]; then
        echo "SUCCESS|$project_dir" > "$temp_result"
        echo "[$(date '+%H:%M:%S')] ✓ SUCCESS: $project_dir"
    else
        echo "ERROR|Build failed|$project_dir" > "$temp_result"
        echo "[$(date '+%H:%M:%S')] ✗ ERROR: $project_dir"
    fi
}

# Export the function and variables so they're available to parallel/xargs
export -f build_project
export PROJECTS_DIR
export OUTPUT_DIR
export TEMP_DIR

# Find all Move.toml files in projects directory, excluding the sui directory
echo "Searching for Move.toml files in projects directory (excluding sui)..."
MOVE_TOMLS=$(find "$PROJECTS_DIR" -type f -name "Move.toml" -not -path "$PROJECTS_DIR/sui/*" | sort)
TOTAL_COUNT=$(echo "$MOVE_TOMLS" | wc -l)

echo "Found $TOTAL_COUNT projects to build"
echo "Starting parallel builds with $PARALLEL_JOBS threads..."
echo ""

# Check if GNU parallel is available
if command -v parallel &> /dev/null; then
    echo "Using GNU parallel for execution..."
    echo "$MOVE_TOMLS" | parallel -j $PARALLEL_JOBS --bar build_project {}
else
    echo "GNU parallel not found, using xargs instead..."
    echo "$MOVE_TOMLS" | xargs -n 1 -P $PARALLEL_JOBS -I {} bash -c 'build_project "$@"' _ {}
fi

# Process results from temp files
echo ""
echo "Processing results..."
SUCCESS_COUNT=0
ERROR_COUNT=0

for result_file in "$TEMP_DIR"/*_result.txt; do
    if [ -f "$result_file" ]; then
        while IFS='|' read -r status details project_path; do
            if [ "$status" = "SUCCESS" ]; then
                echo "✓ SUCCESS: $details" >> "$RESULTS_FILE"
                echo "$details" >> "$SUCCESS_FILE"
                ((SUCCESS_COUNT++))
            elif [ "$status" = "ERROR" ]; then
                echo "✗ ERROR: $details - $project_path" >> "$RESULTS_FILE"
                echo "$project_path" >> "$ERRORS_FILE"
                ((ERROR_COUNT++))
            fi
        done < "$result_file"
    fi
done

# Clean up temp directory
rm -rf "$TEMP_DIR"

# Summary
echo "" | tee -a "$RESULTS_FILE"
echo "========================================" | tee -a "$RESULTS_FILE"
echo "BUILD SUMMARY" | tee -a "$RESULTS_FILE"
echo "========================================" | tee -a "$RESULTS_FILE"
echo "Total projects: $TOTAL_COUNT" | tee -a "$RESULTS_FILE"
echo "Successful builds: $SUCCESS_COUNT" | tee -a "$RESULTS_FILE"
echo "Failed builds: $ERROR_COUNT" | tee -a "$RESULTS_FILE"

# Calculate success rate safely
if [ $TOTAL_COUNT -gt 0 ]; then
    SUCCESS_RATE=$(echo "scale=2; $SUCCESS_COUNT * 100 / $TOTAL_COUNT" | bc)
    echo "Success rate: ${SUCCESS_RATE}%" | tee -a "$RESULTS_FILE"
else
    echo "Success rate: N/A (no projects found)" | tee -a "$RESULTS_FILE"
fi

echo "" | tee -a "$RESULTS_FILE"
echo "Completed at: $(date)" | tee -a "$RESULTS_FILE"
echo "========================================" | tee -a "$RESULTS_FILE"

# List all projects with errors
if [ $ERROR_COUNT -gt 0 ]; then
    echo "" | tee -a "$RESULTS_FILE"
    echo "PROJECTS WITH BUILD ERRORS:" | tee -a "$RESULTS_FILE"
    echo "----------------------------------------" | tee -a "$RESULTS_FILE"
    while IFS= read -r project; do
        echo "• $project" | tee -a "$RESULTS_FILE"
    done < "$ERRORS_FILE"
fi

echo ""
echo "Full results saved in: $OUTPUT_DIR/"
echo "Summary file: $RESULTS_FILE"
echo "Error list: $ERRORS_FILE"
echo "Success list: $SUCCESS_FILE"