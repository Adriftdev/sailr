import re

def fix(path):
    with open(path, "r") as f:
        text = f.read()

    text = text.replace("[service.ci-build-hello]", "[[service]]\n        name = \"ci-build-hello\"")
    text = text.replace("[service.ci-build-hello.build]", "[service.build]")
    
    with open(path, "w") as f:
        f.write(text)

fix("src/workflow/runner.rs")
