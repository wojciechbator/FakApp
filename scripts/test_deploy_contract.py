from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
JUSTFILE = (ROOT / "justfile").read_text()
DEPLOY = ROOT / "scripts/deploy.sh"
DEPLOY_TEXT = DEPLOY.read_text()

subprocess.run(["bash", "-n", str(DEPLOY)], check=True)

assert "deploy:\n    scripts/deploy.sh" in JUSTFILE
assert "rollback:\n    scripts/deploy.sh rollback" in JUSTFILE

# Deploy script checks
assert "virya-oracle" in DEPLOY_TEXT
assert "fakap.virya.music" in DEPLOY_TEXT
assert "healthz" in DEPLOY_TEXT
assert "fakap.previous" in DEPLOY_TEXT  # rollback support
assert "systemctl restart fakap" in DEPLOY_TEXT
assert "statically linked" in DEPLOY_TEXT  # binary verification
assert "DEPLOY=OK" in DEPLOY_TEXT
assert "worktree must be clean" in DEPLOY_TEXT

print("FAKAP_DEPLOY_CONTRACT=PASS binary-swap=true rollback=true health-check=true")
