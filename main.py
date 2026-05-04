import subprocess
import sys

TARGETS = {
    "syn": {"src": "src/synthesizer/main.rs", "out": "synthesizer"},
    "gra": {"src": "src/graph/main.rs", "out": "graph"},
}

def run_cmd(cmd):
    try:
        subprocess.run(cmd, check=True)
    except subprocess.CalledProcessError:
        sys.exit(1)

def build_and_run(target_name):
    if target_name not in TARGETS:
        print(f"Skipping: '{target_name}' (not a valid target)")
        return
    
    target = TARGETS[target_name]
    # Build
    print(f"--- Building {target_name} ---")
    run_cmd(["rustc", target["src"], "-o", target["out"]])
    # Run
    print(f"--- Running {target_name} ---")
    run_cmd([f"./{target['out']}"])

def main():
    args = sys.argv[1:]
    
    if not args:
        print(f"Usage: python script.py <target1> <target2> ... (Available: {list(TARGETS.keys())} or 'all')")
        sys.exit(1)

    tasks = list(TARGETS.keys()) if "all" in args else args

    for task in tasks:
        build_and_run(task)

if __name__ == "__main__":
    main()
