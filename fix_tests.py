import re

planner_path = "src/workflow/planner.rs"
with open(planner_path, "r") as f:
    planner = f.read()

planner = planner.replace("[service.build]\\n        \"#;", "[service.build]\\n        path = \".\"\\n        \"#;")

with open(planner_path, "w") as f:
    f.write(planner)

runner_path = "src/workflow/runner.rs"
with open(runner_path, "r") as f:
    runner = f.read()

runner = runner.replace("[service.build]\\n        \"#;", "[service.build]\\n        path = \".\"\\n        \"#;")

with open(runner_path, "w") as f:
    f.write(runner)

