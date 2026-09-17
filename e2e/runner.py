"""Host orchestration only; every application/seed/browser process runs in Docker."""
import argparse
import base64
import concurrent.futures
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import subprocess
import threading
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / ".e2e"
REGISTRY = json.loads((ROOT / "e2e/families/registry.json").read_text())
FAMILIES = {name: value["seed"] for name, value in REGISTRY.items()}
OWNER = "crm-journey-e2e-v1"
STOP = threading.Event()
ENV = {k: v for k, v in os.environ.items() if k in {
    "PATH", "HOME", "TMPDIR", "DOCKER_HOST", "DOCKER_CONTEXT", "DOCKER_CONFIG",
    "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH", "SSH_AUTH_SOCK"}}


def write_json(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n")


def command(args, log, timeout=300, cancellable=True):
    """Always reap child processes; commands/secret env are never echoed."""
    with log.open("ab") as output:
        child = subprocess.Popen(args, cwd=ROOT, env=ENV, stdout=output,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + timeout
        try:
            while True:
                if cancellable and STOP.is_set():
                    raise InterruptedError("run interrupted")
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("command exceeded its deadline")
                try:
                    result = child.wait(timeout=min(0.25, remaining))
                    if result:
                        raise RuntimeError(f"command failed with exit {result}; see {log.name}")
                    return
                except subprocess.TimeoutExpired:
                    pass
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()


def read_command(args):
    return subprocess.check_output(args, cwd=ROOT, env=ENV, text=True, timeout=30).strip()


def compose(project, private):
    # Explicit config/env prevent automatic discovery of the checkout's .env.
    return ["docker", "compose", "--env-file", "/dev/null", "--project-directory",
            str(private), "-p", project, "-f", str(private / "compose.json")]


def config(project, private, artifacts, images, credentials, family):
    c = credentials
    labels = {"crm.e2e.owner": OWNER, "crm.e2e.project": project, "crm.e2e.family": family}
    health = lambda cmd: {"test": cmd, "interval": "1s", "timeout": "3s", "retries": 60}
    app_env = {
        "DATABASE_URL": f"postgres://crm_app:{c['app']}@postgres:5432/crm",
        "CRM_API_BIND_ADDR": "127.0.0.1:3000", "CRM_SESSION_SECRET": c["session"],
        "CRM_SESSION_COOKIE_SECURE": "false", "CRM_RAW_PAYLOAD_KEY": c["raw"],
        "CENTRIFUGO_HTTP_API_KEY": c["realtime_api"],
        "CENTRIFUGO_TOKEN_HMAC_SECRET": c["realtime_token"],
        "CRM_CENTRIFUGO_API_URL": "http://mocks:9000/api",
        "CRM_INTAKE_MAIL_DOMAIN": "example.test", "RUST_LOG": "info",
        # External providers are disabled; internal network also denies egress.
        "GROQ_API_KEY": "", "LIVEKIT_API_KEY": "", "CRM_FUB_SYSTEM_KEY": "",
        "CRM_FUB_SYSTEM_NAME": "", "CRM_INBOUND_EMAIL_SECRET": "",
    }
    browser_env = {
        "HOME": "/tmp",
        "E2E_FAMILY": family, "E2E_PROJECT": project,
        "E2E_PASSWORD": c["password"], "E2E_SEED": FAMILIES[family],
        "E2E_AUDIT_URL": f"postgres://e2e_audit:{c['audit']}@postgres:5432/crm",
        "E2E_MIGRATION_COUNT": str(len(list((ROOT / "backend/crates/crm-api/migrations").glob("*.sql")))),
    }
    if family in {"routing", "correspondence"}:
        app_env["CRM_INBOUND_EMAIL_SECRET"] = c["inbound"]
        browser_env["E2E_INBOUND_SECRET"] = c["inbound"]
    if family in {"operator", "calls"}:
        app_env.update({"GROQ_API_KEY": c["inference"], "E2E_PROVIDER_PROXY": "1",
                        "CRM_OPERATOR_BASE_URL": "http://127.0.0.1:9001/openai/v1"})
    if family == "calls":
        app_env.update({"LIVEKIT_API_KEY": c["livekit_key"], "LIVEKIT_API_SECRET": c["livekit_secret"],
                        "LIVEKIT_URL": "ws://mocks:9000", "LIVEKIT_API_URL": "http://127.0.0.1:9001",
                        "LIVEKIT_SIP_OUTBOUND_TRUNK_ID": "synthetic-trunk"})
        browser_env.update({"E2E_LIVEKIT_KEY": c["livekit_key"], "E2E_LIVEKIT_SECRET": c["livekit_secret"]})
    services = {
        "postgres": {"image": "postgres:18.6",
            "environment": {"POSTGRES_USER": "postgres", "POSTGRES_PASSWORD": c["root"],
                "POSTGRES_DB": "crm", "CRM_DB_APP_PASSWORD": c["app"],
                "CRM_DB_MIGRATOR_PASSWORD": c["migrator"], "E2E_AUDIT_PASSWORD": c["audit"]},
            "volumes": ["database:/var/lib/postgresql",
                f"{ROOT / 'e2e/provision.sql'}:/docker-entrypoint-initdb.d/roles.sql:ro"],
            "healthcheck": health(["CMD", "pg_isready", "-U", "postgres", "-d", "crm"])},
        "centrifugo": {"image": "centrifugo/centrifugo:v6.9.2",
            "command": ["centrifugo", "--config=/centrifugo/config.json"],
            "environment": {"CENTRIFUGO_HTTP_API_KEY": c["realtime_api"],
                "CENTRIFUGO_CLIENT_TOKEN_HMAC_SECRET_KEY": c["realtime_token"]},
            "volumes": [f"{private / 'centrifugo.json'}:/centrifugo/config.json:ro"],
            "healthcheck": health(["CMD", "wget", "-q", "--spider", "http://127.0.0.1:8000/health"])},
        "mocks": {"image": images["browser"], "command": ["node", "support/mocks.mjs"],
            "environment": {"E2E_PROJECT": project, "E2E_FAMILY": family,
                **({"E2E_LIVEKIT_KEY": c["livekit_key"], "E2E_LIVEKIT_SECRET": c["livekit_secret"]} if family == "calls" else {})},
            "healthcheck": health(["CMD", "node", "-e",
                "fetch('http://127.0.0.1:9000/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"])},
        "migrate": {"image": images["api"], "command": ["migrate"],
            "environment": {"MIGRATION_DATABASE_URL":
                f"postgres://crm_migrator:{c['migrator']}@postgres:5432/crm"}},
        "bootstrap": {"image": images["api"], "command": ["crm-admin",
            "bootstrap-platform-admin", "--email", "platform@e2e.test", "--display-name", "E2E Platform"],
            "environment": {"MIGRATION_DATABASE_URL":
                f"postgres://crm_migrator:{c['migrator']}@postgres:5432/crm",
                "CRM_DEV_SEED_PASSWORD": c["password"]}},
        "api": {"image": images["api"], "environment": app_env,
            "healthcheck": health(["CMD", "curl", "-fsS", "http://127.0.0.1:3000/internal/ready"])},
        "web": {"image": images["web"], "healthcheck": health(["CMD", "wget",
            "-q", "--spider", "http://127.0.0.1:8080/"])},
        "seed": {"image": images["browser"], "command": ["node", "support/seed.mjs"],
            "user": f"{os.getuid()}:{os.getgid()}",
            "environment": browser_env, "volumes": [f"{private}:/private", f"{artifacts}:/artifacts"]},
        "browser": {"image": images["browser"], "environment": browser_env,
            "user": f"{os.getuid()}:{os.getgid()}",
            "volumes": [f"{private}:/private", f"{artifacts}:/artifacts"], "shm_size": "512m"},
    }
    if family == "calls":
        services["web"]["image"] = images["calls-web"]
    if family == "migration":
        services["api"]["image"] = images["migration-api"]
        app_env["E2E_FAMILY"] = family
        services["api"]["healthcheck"] = health(["CMD", "curl", "-fsS", "http://127.0.0.1:3001/internal/ready"])
    for service in services.values():
        service.update({"labels": labels, "init": True})
    return {"services": services,
            "networks": {"default": {"internal": True, "labels": labels}},
            "volumes": {"database": {"labels": labels}}}


def redact_text(text, values):
    for value in sorted(set(values), key=len, reverse=True):
        if len(value) >= 8:
            text = text.replace(value, "[REDACTED]")
    # JWTs may be issued immediately before a process dies, before recording.
    return re.sub(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+", "[REDACTED_JWT]", text)


def sanitize(artifacts, private, credentials):
    values = list(credentials.values())
    secret_file = private / "observed-secrets.json"
    if secret_file.exists():
        values.extend(json.loads(secret_file.read_text()))
    def redact_zip(data):
        result = io.BytesIO()
        with zipfile.ZipFile(io.BytesIO(data)) as source, zipfile.ZipFile(result, "w", zipfile.ZIP_DEFLATED) as dest:
            for entry in source.infolist():
                dest.writestr(entry.filename, redact_bytes(source.read(entry.filename)))
        return result.getvalue()

    def redact_bytes(data):
        if data.startswith(b"PK\x03\x04"):
            return redact_zip(data)
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            return data
        # Playwright HTML embeds its report in a base64 ZIP.
        def embedded(match):
            raw = base64.b64decode(match.group(1))
            return "data:application/zip;base64," + base64.b64encode(redact_zip(raw)).decode()
        text = re.sub(r"data:application/zip;base64,([A-Za-z0-9+/=]+)", embedded, text)
        return redact_text(text, values).encode()

    # Sanitization happens after the browser/log writers stop, before delivery.
    for path in artifacts.rglob("*"):
        if not path.is_file():
            continue
        path.write_bytes(redact_bytes(path.read_bytes()))


def owned_cleanup(project, private, log):
    if not re.fullmatch(r"crm-e2e-[a-f0-9]{12}-[a-z0-9]+-\d+-a\d+", project):
        raise ValueError("invalid E2E project identifier")
    manifest = json.loads((private / "compose.json").read_text())
    if any(s.get("labels", {}).get("crm.e2e.owner") != OWNER or
           s.get("labels", {}).get("crm.e2e.project") != project
           for s in manifest["services"].values()):
        raise ValueError("ownership mismatch; refusing cleanup")
    for kind in ['networks', 'volumes']:
        for resource in manifest.get(kind, {}).values():
            if ('name' in resource or 'external' in resource or
                    resource.get('labels', {}).get('crm.e2e.owner') != OWNER or
                    resource.get('labels', {}).get('crm.e2e.project') != project):
                raise ValueError('explicit or unowned resource declaration; refusing cleanup')
    # Check live resources too: never rely on a user-editable filename alone.
    ids = read_command(["docker", "ps", "-aq", "--filter", f"label=com.docker.compose.project={project}"]).split()
    if ids:
        records = json.loads(read_command(["docker", "inspect", *ids]))
        if any(r["Config"]["Labels"].get("crm.e2e.owner") != OWNER or
               r["Config"]["Labels"].get("crm.e2e.project") != project for r in records):
            raise ValueError("unowned container in project; refusing cleanup")
    for kind in ["network", "volume"]:
        ids = read_command(["docker", kind, "ls", "-q", "--filter",
                            f"label=com.docker.compose.project={project}"]).split()
        if ids:
            records = json.loads(read_command(["docker", kind, "inspect", *ids]))
            if any((r.get("Labels") or {}).get("crm.e2e.owner") != OWNER or
                   (r.get("Labels") or {}).get("crm.e2e.project") != project for r in records):
                raise ValueError(f"unowned {kind} in project; refusing cleanup")
    command(compose(project, private) + ["down", "--volumes", "--remove-orphans", "--timeout", "5"],
            log, timeout=60, cancellable=False)
    remaining = read_command(["docker", "ps", "-aq", "--filter", f"label=com.docker.compose.project={project}"])
    networks = read_command(["docker", "network", "ls", "-q", "--filter", f"label=com.docker.compose.project={project}"])
    volumes = read_command(["docker", "volume", "ls", "-q", "--filter", f"label=com.docker.compose.project={project}"])
    if remaining or networks or volumes:
        raise RuntimeError("owned resources remain after cleanup")


def attempt(run_id, family, copy, number, images, revision, options):
    project = f"crm-e2e-{run_id}-{family}-{copy}-a{number}"
    private = STATE / "private" / project
    artifacts = STATE / "runs" / run_id / f"{family}-{copy}-a{number}"
    private.mkdir(parents=True, mode=0o700)
    artifacts.mkdir(parents=True, mode=0o700)
    credentials = {k: secrets.token_hex(24 if k == "password" else 32)
                   for k in ["app", "root", "migrator", "audit", "session", "raw", "password",
                             "realtime_api", "realtime_token", "inbound", "inference", "livekit_key", "livekit_secret"]}
    write_json(private / "centrifugo.json", {"log": {"level": "info"}, "health": {"enabled": True},
        "client": {"allowed_origins": ["http://web:8080"]}, "channel": {"namespaces": [{"name": "org"}]}})
    write_json(private / "compose.json", config(project, private, artifacts, images, credentials, family))
    for file in private.iterdir():
        file.chmod(0o600)
    cmd = compose(project, private)
    report = {"project": project, "family": family, "copy": copy, "attempt": number,
              "seed": FAMILIES[family], "revision": revision, "images": images,
              "started_at": time.time(), "status": "setup", "cleanup": "pending"}
    deadline = time.monotonic() + options.timeout
    def run(args):
        command(cmd + args, artifacts / "runner.log", timeout=max(1, deadline - time.monotonic()))
    try:
        run(["up", "-d", "--wait", "postgres", "centrifugo", "mocks"])
        run(["run", "--rm", "migrate"])
        run(["run", "--rm", "bootstrap"])
        run(["up", "-d", "--wait", "api", "web"])
        run(["run", "--rm", "seed"])
        # Record only IDs/network membership, never inspect environment secrets.
        ids = read_command(cmd + ["ps", "-q"]).split()
        records = json.loads(read_command(["docker", "inspect", *ids]))
        report["containers"] = [{"id": r["Id"], "image_id": r["Image"],
            "service": r["Config"]["Labels"]["com.docker.compose.service"],
            "networks": {k: v["NetworkID"] for k, v in r["NetworkSettings"]["Networks"].items()}}
            for r in records]
        if options.fail_step:
            config_path = private / "compose.json"
            manifest = json.loads(config_path.read_text())
            manifest["services"]["browser"]["environment"]["E2E_FAIL_STEP"] = str(options.fail_step)
            write_json(config_path, manifest)
        run(["up", "-d", "browser"])
        browser = read_command(cmd + ["ps", "-aq", "browser"])
        run_wait_log = artifacts / "browser-exit.log"
        command(["docker", "wait", browser], run_wait_log, timeout=max(1, deadline - time.monotonic()))
        if run_wait_log.read_text().strip() != "0":
            raise RuntimeError("browser family failed; see Playwright report")
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "interrupted" if STOP.is_set() else "failed"
        report["error"] = str(error)
    finally:
        report["finished_at"] = time.time()
        # Stop browser writers before collecting/sanitizing. Diagnostics precede teardown.
        try:
            command(cmd + ["stop", "--timeout", "10", "browser"], artifacts / "runner.log",
                    timeout=20, cancellable=False)
            command(cmd + ["logs", "--no-color", "--timestamps"], artifacts / "services.log",
                    timeout=30, cancellable=False)
            if read_command(cmd + ['ps', '-q', 'mocks']):
                command(cmd + ['exec', '-T', 'mocks', 'node', '-e',
                    "fetch('http://127.0.0.1:9000/evidence').then(r=>r.text()).then(console.log)"],
                    artifacts / 'mock-requests.json', timeout=10, cancellable=False)
        except Exception as error:
            report["diagnostic_error"] = str(error)
            report["status"] = "failed"
        keep = options.keep_failed and report["status"] != "passed"
        try:
            if keep:
                report["cleanup"] = "retained_by_request"
                report["cleanup_command"] = f"./scripts/e2e --cleanup {project}"
            else:
                owned_cleanup(project, private, artifacts / "cleanup.log")
                report["cleanup"] = "verified_empty"
        except Exception as error:
            report["cleanup"] = "failed"
            report["status"] = "failed"
            report["cleanup_error"] = str(error)
        try:
            sanitize(artifacts, private, credentials)
            if not keep and report["cleanup"] == "verified_empty":
                shutil.rmtree(private)
        except Exception as error:
            report["status"] = "failed"
            report["sanitization_error"] = str(error)
            # Do not advertise raw artifacts as safe if sanitization failed.
            print(f"{project}: artifact sanitization failed; artifacts are private", flush=True)
        # Even setup failures have an explicit blocked browser-family outcome.
        if not (artifacts / "steps.json").exists():
            write_json(artifacts / "steps.json", [{"name": name, "status": "blocked",
                "reason": report.get("error", "setup incomplete")} for name in REGISTRY[family]["steps"]])
        else:
            steps = json.loads((artifacts / 'steps.json').read_text())
            for step in steps:
                if step['status'] == 'running':
                    step.update(status='failed', error=report.get('error', 'browser stopped before step completed'))
            write_json(artifacts / 'steps.json', steps)
        write_json(artifacts / "run.json", report)
        print(f"{project}: {report['status']} ({report['cleanup']}) — {artifacts}", flush=True)
        if keep:
            print(report["cleanup_command"], flush=True)
    return report


BUILD_INPUTS = {
    'api': ['backend/Cargo.toml', 'backend/Cargo.lock', 'backend/.sqlx',
            'backend/crates', 'e2e/api-start.sh'],
    'web': ['web/package.json', 'web/pnpm-lock.yaml', 'web/pnpm-workspace.yaml',
            'web/index.html', 'web/vite.config.ts', 'web/tsconfig*.json',
            'web/src', 'web/public', 'e2e/nginx.conf'],
    'browser': ['e2e/package.json', 'e2e/package-lock.json',
                'e2e/playwright.config.mjs', 'e2e/families', 'e2e/support'],
}

BUILD_INPUTS['calls-web'] = BUILD_INPUTS['web'] + ['e2e/calls-vite.config.mjs', 'e2e/livekit-browser.mjs']
BUILD_INPUTS['migration-api'] = BUILD_INPUTS['api'] + ['e2e/migration-server.rs']


def build_targets(families):
    return ['api', 'web', 'browser'] + (['calls-web'] if 'calls' in families else []) + (['migration-api'] if 'migration' in families else [])


def image_fingerprint(target, root=ROOT):
    """Hash the target's Docker COPY inputs, including untracked source changes."""
    files = set()
    excluded = {'node_modules', 'target', 'test-results', 'playwright-report', '__pycache__'}
    for pattern in BUILD_INPUTS[target] + ['e2e/Dockerfile', 'e2e/Dockerfile.dockerignore']:
        for path in root.glob(pattern):
            for item in path.rglob('*') if path.is_dir() else [path]:
                relative = item.relative_to(root)
                if any(p in excluded or p.startswith('.env') for p in relative.parts):
                    continue
                if item.is_file() or item.is_symlink():
                    files.add(relative)
    digest = hashlib.sha256(('crm-e2e-image-v1:' + target).encode())
    for relative in sorted(files):
        path = root / relative
        payload = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        digest.update(json.dumps([str(relative), path.lstat().st_mode & 0o777,
                                 path.is_symlink(), hashlib.sha256(payload).hexdigest()]).encode())
    return digest.hexdigest()


def build(run_dir, rebuild=False, targets=None):
    images, evidence = {}, {}
    cache = STATE / 'image-locks'
    cache.mkdir(parents=True, exist_ok=True)
    for target in targets or ["api", "web", "browser"]:
        key = image_fingerprint(target)
        tag = f'crm-e2e-{target}:inputs-{key}'
        # A process lock avoids duplicate builds across simultaneous invocations.
        # The OS releases it on interruption; mutable journey state is never cached.
        with (cache / f'{target}-{key}.lock').open('a') as lock:
            deadline = time.monotonic() + 1800
            while True:
                if STOP.is_set():
                    raise InterruptedError('interrupted while waiting for image build')
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if time.monotonic() >= deadline:
                        raise TimeoutError('timed out waiting for image build')
                    STOP.wait(.25)
            existing = subprocess.run(['docker', 'image', 'inspect', '-f', '{{.Id}}', tag],
                                      env=ENV, capture_output=True, text=True, timeout=30)
            reused = existing.returncode == 0 and not rebuild
            if reused:
                image = existing.stdout.strip()
                print(f'Reusing {target}: {image[:19]}', flush=True)
            else:
                print(f"Building {target}; log: {run_dir / ('build-' + target + '.log')}", flush=True)
                command(['docker', 'build', '-f', 'e2e/Dockerfile', '--target', target, '-t', tag, '.'],
                        run_dir / f'build-{target}.log', timeout=1800)
                if image_fingerprint(target) != key:
                    # Never publish an image under a key for inputs that changed mid-build.
                    command(['docker', 'image', 'rm', tag], run_dir / f'build-{target}.log', cancellable=False)
                    raise RuntimeError('build inputs changed during image build; rerun with a stable checkout')
                image = read_command(['docker', 'image', 'inspect', '-f', '{{.Id}}', tag])
            images[target] = image
            evidence[target] = {'input_sha256': key, 'image_id': image, 'reused': reused}
            write_json(run_dir / 'build-cache.json', evidence)
    return images


def isolation_proof(run_dir, results):
    """Compare actual successful environments, including overlapping execution."""
    successful = [r for r in results if r['status'] == 'passed']
    pairs = []
    for i, left in enumerate(successful):
        for right in successful[i + 1:]:
            if (left['family'], left['copy']) == (right['family'], right['copy']):
                continue
            def data(report):
                path = run_dir / f"{report['family']}-{report['copy']}-a{report['attempt']}"
                return json.loads((path / 'isolation.json').read_text())
            a, b = data(left), data(right)
            nets = lambda r: {v for c in r['containers'] for v in c['networks'].values()}
            checks = {'networks_disjoint': not nets(left) & nets(right),
                      'organizations_disjoint': not set(a['organizations']) & set(b['organizations']),
                      'people_disjoint': not set(a['people']) & set(b['people']),
                      'project_markers_match': a['project'] == left['project'] and b['project'] == right['project']}
            if not all(checks.values()):
                raise RuntimeError('concurrent environment isolation check failed')
            pairs.append({'projects': [left['project'], right['project']], **checks,
                'execution_overlapped': max(left['started_at'], right['started_at']) <
                                       min(left['finished_at'], right['finished_at'])})
    write_json(run_dir / 'isolation-proof.json', {'pairs': pairs,
        'concurrency_demonstrated': any(p['execution_overlapped'] for p in pairs)})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--family", action="append", choices=FAMILIES)
    parser.add_argument("--all", action="store_true", help="all implemented families")
    parser.add_argument("--copies", type=int, default=1, help="independent copies for isolation proof")
    parser.add_argument("--concurrency", type=int, default=2)
    parser.add_argument("--retries", type=int, default=0, help="fresh whole-family attempts")
    parser.add_argument("--timeout", type=int, default=420, help="seconds per family, after builds")
    parser.add_argument("--keep-failed", action="store_true")
    parser.add_argument("--fail-step", type=int, help="diagnostics self-check: deliberately fail this numbered browser step")
    parser.add_argument("--cleanup")
    parser.add_argument("--images", type=Path, help="reuse an earlier run's immutable images.json")
    parser.add_argument('--rebuild', action='store_true', help='invoke Docker builds even when input-matched images exist; Docker layers remain cached')
    args = parser.parse_args()
    if args.images and args.rebuild:
        parser.error('--images and --rebuild cannot be combined')
    if args.cleanup:
        private = STATE / "private" / args.cleanup
        owned_cleanup(args.cleanup, private, STATE / "cleanup.log")
        shutil.rmtree(private)
        print("Verified owned containers, network and volumes removed.")
        return 0
    if min(args.copies, args.concurrency, args.timeout) < 1 or args.retries < 0:
        parser.error("counts/timeouts must be positive; retries must be nonnegative")
    families = list(FAMILIES) if args.all else list(dict.fromkeys(args.family or ["leads"]))
    STOP.clear()
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: STOP.set())
    run_id = uuid.uuid4().hex[:12]
    run_dir = STATE / "runs" / run_id
    run_dir.mkdir(parents=True, mode=0o700)
    revision = read_command(["git", "rev-parse", "HEAD"])
    # Include untracked harness files, without reading ignored secrets/build outputs.
    patch = read_command(["git", "diff", "HEAD"])
    paths = read_command(['git', 'ls-files', '--cached', '--others', '--exclude-standard',
                          'e2e', 'scripts/e2e', 'backend', 'web']).splitlines()
    fingerprints = {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest()
                    for p in paths if (ROOT / p).is_file()}
    write_json(run_dir / "source.json", {"revision": revision, "tracked_diff_sha256":
        hashlib.sha256(patch.encode()).hexdigest(), "inputs": fingerprints,
        "images_reused_from": str(args.images) if args.images else None,
        "status": read_command(["git", "status", "--short"])})
    try:
        images = json.loads(args.images.read_text()) if args.images else build(run_dir, args.rebuild, build_targets(families))
        missing = set(build_targets(families)) - set(images)
        if missing:
            raise ValueError("Image manifest is missing selected targets: " + ", ".join(sorted(missing)))
        # Resolve tags once to immutable IDs before any concurrent family starts.
        images = {k: read_command(["docker", "image", "inspect", "-f", "{{.Id}}", v]) for k, v in images.items()}
        write_json(run_dir / "images.json", images)
        def execute(family, copy):
            results = []
            for n in range(1, args.retries + 2):
                if STOP.is_set():
                    break
                result = attempt(run_id, family, copy, n, images, revision, args)
                results.append(result)
                if result["status"] == "passed" or result["cleanup"] != "verified_empty":
                    break
            return results
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
            futures = [pool.submit(execute, family, copy)
                       for family in families for copy in range(1, args.copies + 1)]
            results = [result for future in futures for result in future.result()]
        write_json(run_dir / "summary.json", results)
        isolation_proof(run_dir, results)
        print(f"Run evidence: {run_dir}", flush=True)
        latest = {(r["family"], r["copy"]): r for r in results}
        return 0 if len(latest) == len(families) * args.copies and all(
            r["status"] == "passed" and r["cleanup"] == "verified_empty" for r in latest.values()) else 1
    except Exception as error:
        write_json(run_dir / "setup-failure.json", {"error": str(error)})
        print(f"Setup failed: {error}; evidence: {run_dir}", file=__import__("sys").stderr)
        return 1
