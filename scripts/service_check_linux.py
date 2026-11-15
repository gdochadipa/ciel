"""Exercise the real offline CLI in a disposable, network-disabled Linux container.

This suite needs root solely for synthetic fixture ownership and credential-drop
checks. It never edits host accounts or uses real Google credentials. Runtime cases
start a disposable Core against loopback synthetic Google. Invoke through the adjacent
acceptance launcher, not Python unittest discovery on an operator's host.
"""

import ctypes
import hashlib
import json
import os
import platform
from pathlib import Path
import select
import shutil
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import tarfile
import unittest
from unittest.mock import patch
import http.server
import threading
import urllib.request
import urllib.error


SOURCE = Path(__file__).resolve().parent.parent
ARTIFACTS = (
    "LICENSE", "scripts/preflight.py", "scripts/backup.py",
    "templates/service.env.example", "templates/qualification.example.json",
    "templates/registry.example.json", "templates/openclaw.example.json",
    "templates/secretary-core.service", "INSTALLATION.md",
    "bin/secretary-core", "plugins/secretary-tools.tgz",
)
SECRET_FIELDS = (
    "owner_file", "core_token_file", "operator_token_file",
    "google_credentials_file", "budget_policy_file",
)
BLOCKERS = [
    "budget_policy_runtime_validation_pending", "launcher_authority_pending",
    "release_contract_and_compatibility_pending", "runtime_secret_validation_pending",
]
CANARY = b"SYNTHETIC_UNREAD_BODY_CANARY"
# Offline release checks hash the real SDK/artifact tree on a disposable filesystem.
# This fixture budget is separate from production admission and task deadlines.
READONLY_COMMAND_TIMEOUT = 60
_READONLY_TIMEOUT_EXIT = 124
_READONLY_SUPERVISOR = """\
import subprocess
import sys
try:
    result = subprocess.run(sys.argv[2:], env={}, timeout=float(sys.argv[1]))
except subprocess.TimeoutExpired:
    sys.exit(124)
sys.exit(result.returncode)
"""


def acl_bytes():
    """A valid named-user ACL, encoded using Linux's stable xattr ABI."""
    entries = ((1, 6, 0xFFFFFFFF), (2, 4, 65534), (4, 4, 0xFFFFFFFF),
               (16, 4, 0xFFFFFFFF), (32, 0, 0xFFFFFFFF))
    return struct.pack("<I", 2) + b"".join(struct.pack("<HHI", *e) for e in entries)


class ServiceCheckLinux(unittest.TestCase):
    binary = None

    @classmethod
    def setUpClass(cls):
        target = Path(cls.binary).parent
        arguments = ["rustc", "--edition=2021", "-D", "warnings",
                     str(SOURCE / "services/secretary-core/tests/support/launcher_probe.rs"),
                     "-L", f"dependency={target / 'deps'}"]
        for name in ("secretary_core", "libc", "tokio", "axum"):
            libraries = list((target / "deps").glob(f"lib{name}-*.rlib"))
            if len(libraries) != 1:
                raise RuntimeError("Acceptance requires a clean, unambiguous native build directory")
            arguments.extend(["--extern", f"{name}={libraries[0]}"])
        cls.launcher_probe = str(target / "launcher-probe")
        subprocess.run([*arguments, "-o", cls.launcher_probe], check=True, timeout=60)
        cls.gateway_payload = str(target / "gateway-payload")
        libc_library = list((target / "deps").glob("liblibc-*.rlib"))[0]
        subprocess.run(["rustc", "--edition=2021", "-D", "warnings", "-C", "opt-level=1",
                        str(SOURCE / "services/secretary-core/tests/support/gateway_payload.rs"),
                        "-L", f"dependency={target / 'deps'}", "--extern", f"libc={libc_library}",
                        "-o", cls.gateway_payload], check=True, timeout=60)

    def setUp(self):
        # /tmp is intentionally forbidden by the checker. Create an administrator-
        # owned tree on the disposable container's native Linux filesystem instead.
        self.root = Path(tempfile.mkdtemp(prefix="ciel-service-check-", dir="/"))
        self.addCleanup(shutil.rmtree, self.root)
        self.root.chmod(0o755)
        self.config = self.root / "config"
        self.config.mkdir(mode=0o755)
        self.state = self.root / "state"
        self.state.mkdir(mode=0o700)
        # These pinned Debian container accounts are separate and have exclusive
        # primary groups; no account database is modified to make the test pass.
        os.chown(self.state, 1, 1)
        self.manifest = json.loads((SOURCE / "config/readonly-manifest.example.json").read_text())
        for field in (*SECRET_FIELDS, "registry_file", "service_policy_file"):
            self.manifest[field] = str(self.config / Path(self.manifest[field]).name)
        self.manifest["state_directory"] = str(self.state)
        self.manifest["release_file"] = str(self.root / "release/release.json")
        self.manifest_path = self.config / "manifest.json"
        self.write_json(self.manifest_path, self.manifest)
        policy = json.loads((SOURCE / "config/service-policy.example.json").read_text())
        policy["core"] = {"uid": 1, "gid": 1}
        policy["gateway"] = {"uid": 65534, "gid": 65534}
        self.write_json(self.manifest["service_policy_file"], policy)
        registry = json.loads((SOURCE / "config/workspace.example.json").read_text())
        self.write_json(self.manifest["registry_file"], registry)
        for field in SECRET_FIELDS:
            self.write(self.manifest[field], CANARY)
        hashes = {}
        for name in ARTIFACTS:
            path = self.root / "release" / name
            path.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
            content = b"synthetic release inventory bytes: " + name.encode()
            self.write(path, content, 0o755 if name == "bin/secretary-core" else 0o644)
            hashes[name] = hashlib.sha256(content).hexdigest()
        self.write_json(self.manifest["release_file"], {
            "format_version": 2, "source_commit": "a" * 40,
            "build_host": {"system": "Linux", "machine": "aarch64", "libc": ["glibc", "2.36"]},
            "tool_versions": {"rustc": "synthetic"}, "tool_count": 33,
            "production_writes": False, "starts_services": False, "activates_plugins": False,
            "lock_hashes": {"Cargo.lock": "b" * 64, "bun.lock": "c" * 64},
            "artifact_hashes": hashes,
        }, 0o644)

    def compatible_release(self):
        root = self.root / "release"
        installed_core = root / "bin/secretary-core"
        archive = Path(self.binary).parent / "readonly-plugin.tgz"
        shutil.copyfile(self.binary, installed_core)
        shutil.copyfile(archive, root / "plugins/secretary-tools.tgz")
        for name, source in {
            "service.env.example": ".env.example",
            "qualification.example.json": "deploy/qualification.example.json",
            "registry.example.json": "config/workspace.example.json",
            "openclaw.example.json": "config/openclaw.example.json",
            "secretary-core.service": "deploy/secretary-core.service",
        }.items():
            shutil.copyfile(SOURCE / source, root / "templates" / name)
        plugin = self.root / "installed-plugin"
        (plugin / "dist").mkdir(parents=True, mode=0o755)
        with tarfile.open(archive) as packed:
            for name in ("package.json", "openclaw.plugin.json", "dist/index.js"):
                member = packed.extractfile(f"package/{name}")
                self.write(plugin / name, member.read(), 0o644)
        machine = platform.machine()
        def update(receipt):
            receipt["build_host"] = {
                "system": "Linux", "machine": machine,
                "libc": os.confstr("CS_GNU_LIBC_VERSION").split(),
            }
            receipt["tool_versions"] = {
                "node": "v24.22.1", "bun": "1.2.21",
                "rustc": "rustc 1.95.0 (test)", "cargo": "cargo 1.95.0 (test)",
                "rust_host": f"{machine}-unknown-linux-gnu", "openclaw": "2026.10.7",
            }
            receipt["lock_hashes"] = {
                name: hashlib.sha256((SOURCE / name).read_bytes()).hexdigest()
                for name in ("Cargo.lock", "bun.lock")
            }
            receipt["artifact_hashes"] = {
                name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in ARTIFACTS
            }
        self.change_json("release_file", update)
        return installed_core, plugin

    def readonly_command(self, executable, action, plugin, *, gateway=None, trailing=()):
        invocation = [
            str(executable),
            "service", action, "--manifest", str(self.manifest_path),
            *(["--gateway-plan", str(gateway)] if gateway else []),
            "--plugin-dir", str(plugin), *trailing,
        ]
        # Root deliberately lacks CAP_KILL here. The timer must share the Core's
        # dropped identity, so timeout cleanup never needs a cross-UID signal.
        command = [
            *self.probe_arguments(self.manifest_path)[:-2], sys.executable,
            "-I", "-B", "-S", "-c", _READONLY_SUPERVISOR,
            str(READONLY_COMMAND_TIMEOUT), *invocation,
        ]
        result = subprocess.run(command, env={}, capture_output=True)
        if result.returncode == _READONLY_TIMEOUT_EXIT:
            raise subprocess.TimeoutExpired(invocation, READONLY_COMMAND_TIMEOUT,
                                            output=result.stdout, stderr=result.stderr)
        return result

    def test_readonly_timeout_is_reaped_under_dropped_identity(self):
        executable = self.root / "slow-cli"
        self.write(executable, (
            f"#!{sys.executable}\nimport os, time\n"
            "print(os.getpid(), flush=True)\ntime.sleep(4)\n"
        ).encode(), 0o755)
        with patch.object(sys.modules[__name__], "READONLY_COMMAND_TIMEOUT", 2):
            with self.assertRaises(subprocess.TimeoutExpired) as raised:
                self.readonly_command(executable, "check-release-readonly", self.root)
        self.assertEqual(raised.exception.timeout, 2)
        pid = int(raised.exception.output.strip())
        # A live cross-UID child would return EPERM here without CAP_KILL; ESRCH
        # proves the same-identity supervisor killed and reaped the fixture.
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)

    def test_release_compatibility_binds_running_inode_actual_sdk_files_and_exact_tree(self):
        core, plugin = self.compatible_release()
        watch = self.watch_secret_inodes()
        result = self.readonly_command(core, "check-release-readonly", plugin)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b"", b""))
        result = self.readonly_command(self.binary, "check-release-readonly", plugin)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        extra = self.root / "release/extra"
        extra.mkdir()
        result = self.readonly_command(core, "check-release-readonly", plugin)
        self.assertEqual(result.returncode, 1)
        extra.rmdir()
        entry = plugin / "dist/index.js"
        entry.write_bytes(b"changed fixture")
        result = self.readonly_command(core, "check-release-readonly", plugin)
        self.assertEqual(result.returncode, 1)
        with self.assertRaises(BlockingIOError):
            os.read(watch, 65536)
        self.assertEqual(list(self.state.iterdir()), [])

    def test_release_compatibility_rejects_wrong_locks_and_rehashed_active_template(self):
        core, plugin = self.compatible_release()
        receipt = Path(self.manifest["release_file"])
        original = receipt.read_bytes()
        self.change_json("release_file", lambda r: r["lock_hashes"].update({"bun.lock": "a" * 64}))
        result = self.readonly_command(core, "check-release-readonly", plugin)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        self.write(receipt, original, 0o644)
        name = "templates/qualification.example.json"
        active = b'{"qualified":true}'
        self.write(self.root / "release" / name, active, 0o644)
        self.change_json("release_file", lambda r: r["artifact_hashes"].update({
            name: hashlib.sha256(active).hexdigest(),
        }))
        result = self.readonly_command(core, "check-release-readonly", plugin)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        self.assertEqual(list(self.state.iterdir()), [])

    def test_admission_denies_missing_promotion_and_invalid_plan_before_secrets(self):
        core, plugin = self.compatible_release()
        plan, _ = self.gateway_plan()
        denied = self.config / "invalid-qualification.json"
        self.write_json(denied, {"schema": "unsupported"})
        initial_state = {
            path.name: (path.read_bytes(), path.stat().st_mode)
            for path in self.state.iterdir()
        }
        watch = self.watch_secret_inodes()
        for action, arguments in (
            ("start-readonly", ("--promotion", str(self.config / "missing.json"))),
            ("qualify-readonly", (
                "--plan", str(denied), "--output", str(self.root / "qualification.json"),
                "--supervisor-pid", str(os.getpid()),
            )),
        ):
            result = self.readonly_command(core, action, plugin, gateway=plan, trailing=arguments)
            self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        with self.assertRaises(BlockingIOError):
            os.read(watch, 65536)
        self.assertEqual({
            path.name: (path.read_bytes(), path.stat().st_mode)
            for path in self.state.iterdir()
        }, initial_state)
        self.assertFalse((self.root / "qualification.json").exists())

    def write(self, path, content, mode=0o640):
        path = Path(path)
        path.write_bytes(content)
        os.chown(path, 0, 1)
        path.chmod(mode)

    def write_json(self, path, value, mode=0o640):
        self.write(path, json.dumps(value).encode(), mode)

    def change_json(self, field, change):
        path = self.manifest[field]
        value = json.loads(Path(path).read_text())
        change(value)
        self.write_json(path, value)

    def snapshot(self):
        result = {}
        for path in [self.root, *sorted(self.root.rglob("*"))]:
            info = path.lstat()
            content = path.read_bytes() if stat.S_ISREG(info.st_mode) else None
            result[str(path.relative_to(self.root))] = (
                info.st_mode, info.st_uid, info.st_gid, info.st_ino,
                info.st_nlink, info.st_mtime_ns, info.st_ctime_ns, content,
                os.readlink(path) if path.is_symlink() else None,
                tuple((name, os.getxattr(path, name, follow_symlinks=False))
                      for name in sorted(os.listxattr(path, follow_symlinks=False))),
            )
        return result

    def check(self, code=None, env=None, as_core=False):
        before = self.snapshot()
        result = subprocess.run(
            [self.binary, "service", "check", "--manifest", str(self.manifest_path)],
            env=env or {}, capture_output=True, timeout=5, check=False,
            **({"user": 1, "group": 1, "extra_groups": []} if as_core else {}),
        )
        self.assertEqual(self.snapshot(), before, "offline CLI changed fixture state")
        self.assertEqual(result.returncode, 3 if code is None else 1, result.stdout)
        self.assertEqual(result.stderr, b"")
        report = json.loads(result.stdout)
        self.assertFalse(report["ok"])
        self.assertEqual(report["readiness"], "incomplete" if code is None else "denied")
        for key in ("configuration_changed", "activation_changed", "live_access_checked",
                    "service_started", "journal_opened"):
            self.assertIs(report[key], False)
        for private in (CANARY, str(self.root).encode(), b"generic-spreadsheet",
                        b"demo-document", b"SYNTHETIC_ENV_CANARY"):
            self.assertNotIn(private, result.stdout)
        if code is not None:
            self.assertEqual(report["error"]["code"], code)
        else:
            self.assertEqual(report["blockers"], BLOCKERS)
            for name in ("manifest", "service_policy", "file_metadata", "local_account_groups",
                         "registry", "release_hashes"):
                self.assertEqual(report["checks"][name], "passed")
            self.assertEqual(report["summary"], {"readable_spreadsheets": 1, "readable_documents": 1})
        return report

    def test_static_snapshot_completes_without_admitting_or_changing_state(self):
        self.check()

    def test_non_root_core_can_complete_the_same_static_snapshot(self):
        self.check(as_core=True)

    def probe_arguments(self, path, groups=None, mode=None, clean=True):
        arguments = [self.launcher_probe, str(path)]
        if mode:
            arguments.append(mode)
        if clean:
            arguments = ["/usr/bin/setpriv", "--bounding-set=-all", "--reuid=1", "--regid=1",
                         *([f"--groups={groups}"] if groups else ["--clear-groups"]),
                         *arguments]
        return arguments

    def launcher(self, expected="passed", groups=None, mode=None, clean=True, as_core=False):
        arguments = self.probe_arguments(self.manifest["service_policy_file"], groups, mode, clean)
        result = subprocess.run(
            arguments, env={}, capture_output=True, timeout=5, check=False,
            **({"user": 1, "group": 1, "extra_groups": []} if as_core else {}),
        )
        self.assertEqual(result.returncode, 0 if expected == "passed" else 1, result.stderr)
        self.assertEqual(result.stdout, (expected + "\n").encode())
        self.assertEqual(result.stderr, b"")

    def gateway_plan(self, threaded=False):
        directory = self.root / "launch"
        directory.mkdir(mode=0o750)
        os.chown(directory, 0, 1)
        home = self.root / "gateway-home"
        home.mkdir(mode=0o700)
        os.chown(home, 65534, 65534)
        executable = self.root / "gateway"
        shutil.copyfile(self.gateway_payload, executable)
        executable.chmod(0o755)
        value = {
            "schema": "ciel.readonly.gateway-plan.v1",
            "deployment_generation": self.manifest["deployment_generation"],
            "manifest_sha256": hashlib.sha256(self.manifest_path.read_bytes()).hexdigest(),
            "policy_sha256": hashlib.sha256(Path(self.manifest["service_policy_file"]).read_bytes()).hexdigest(),
            "executable": str(executable),
            "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            "arguments": ["gateway-payload-threaded" if threaded else "gateway-payload",
                          *[self.manifest[field] for field in SECRET_FIELDS],
                          str(self.state / "private-state")],
            "environment": {"HOME": str(home), "LANG": "C"},
            "working_directory": str(home),
            "intent_file": str(directory / "intent.json"),
        }
        # The launched payload must observe kernel denial, not absent fixtures.
        (self.state / "private-state").write_bytes(CANARY)
        os.chown(self.state / "private-state", 1, 1)
        (self.state / "private-state").chmod(0o600)
        path = self.config / "gateway-plan.json"
        self.write_json(path, value)
        return path, value

    def gateway_arguments(self, plan):
        return [self.binary, "service", "gateway-exec", "--manifest", str(self.manifest_path),
                "--plan", str(plan)]

    def start_gateway(self, plan):
        child = subprocess.Popen(self.gateway_arguments(plan), stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 env={"ROOT_ENV_CANARY": "must-not-survive-exec"})

        def cleanup():
            if child.poll() is None:
                try:
                    child.stdin.write(b"x")
                    child.stdin.flush()
                    child.wait(timeout=3)
                except (BrokenPipeError, subprocess.TimeoutExpired):
                    child.kill()
                    child.wait(timeout=3)
            child.stdin.close()
            child.stdout.close()
            child.stderr.close()

        self.addCleanup(cleanup)
        ready, _, _ = select.select([child.stdout], [], [], 5)
        self.assertTrue(ready, "Gateway payload did not become ready")
        line = child.stdout.readline()
        if line != b"gateway_ready\n":
            child.wait(timeout=3)
            self.fail(f"Gateway launch failed: exit={child.returncode}, stderr={child.stderr.read()!r}")
        return child

    def check_gateway(self, plan, expected="passed"):
        result = subprocess.run([*self.probe_arguments(self.manifest_path, mode="snapshot-gateway"),
                                 str(plan)], env={}, capture_output=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 0 if expected == "passed" else 1, result.stderr)
        self.assertEqual(result.stdout, (expected + "\n").encode())
        self.assertEqual(result.stderr, b"")

    def test_gateway_exec_drops_authority_private_access_and_inherited_environment(self):
        plan, value = self.gateway_plan()
        watch = self.watch_secret_inodes()
        child = self.start_gateway(plan)
        intent = Path(value["intent_file"])
        info = intent.stat()
        self.assertEqual((info.st_uid, info.st_gid, stat.S_IMODE(info.st_mode)), (0, 1, 0o440))
        self.assertEqual(json.loads(intent.read_bytes())["pid"], child.pid)
        self.check_gateway(plan)
        with self.assertRaises(BlockingIOError):
            os.read(watch, 65536)

    def test_gateway_exec_allows_runtime_threads_only_after_guarded_exec(self):
        plan, _ = self.gateway_plan(threaded=True)
        self.start_gateway(plan)
        self.check_gateway(plan)

    def runtime_secrets(self):
        # Public disposable signing fixture, never a Google account credential.
        self.write(self.manifest["owner_file"], b" exact synthetic owner \n")
        self.write(self.manifest["core_token_file"], b"SYNTHETIC-CORE-0123456789-abcdefgh\n")
        self.write(self.manifest["operator_token_file"], b"SYNTHETIC-OPERATOR-9876543210-zyxw\n")
        self.write_json(self.manifest["google_credentials_file"], {
            "type": "service_account", "client_email": "synthetic@test-project.iam.gserviceaccount.com",
            "private_key": (SOURCE / "services/secretary-core/tests/support/synthetic-rsa.pem").read_text(),
            "token_uri": "https://oauth2.googleapis.com/token", "universe_domain": "googleapis.com",
        })
        self.write_json(self.manifest["budget_policy_file"], json.loads((SOURCE / "config/read-budget-policy.example.json").read_text()))

    def runtime_check(self, plan, expected="passed", env=None, clean=True):
        result = subprocess.run(
            [*self.probe_arguments(self.manifest_path, mode="runtime", clean=clean), str(plan)],
            env=env or {}, capture_output=True, timeout=10, check=False)
        self.assertEqual(result.returncode, int(expected != "passed"), result.stderr)
        self.assertEqual(result.stdout, (expected + "\n").encode())
        self.assertEqual(result.stderr, b"")

    def test_runtime_secrets_prepare_same_protected_inputs_without_changing_state(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        before = self.snapshot()
        self.runtime_check(plan)
        self.assertEqual(self.snapshot(), before)

    def test_runtime_invalid_tokens_identity_key_and_duplicate_credentials_are_sanitized(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        for field, content, expected in (
            ("owner_file", b"owner\n\n", "runtime_owner_invalid"),
            ("core_token_file", b"short", "runtime_token_invalid"),
            ("core_token_file", b"replace-with-a-random-token-at-least-32-bytes", "runtime_token_invalid"),
            ("operator_token_file", b"SYNTHETIC-CORE-0123456789-abcdefgh", "runtime_tokens_not_distinct"),
            ("google_credentials_file", b'{"type":"service_account","type":"PRIVATE_CANARY"}', "runtime_credentials_invalid"),
            ("google_credentials_file", b"x" * 65537, "runtime_secret_too_large"),
        ):
            with self.subTest(field=field, expected=expected):
                self.runtime_secrets()
                self.write(self.manifest[field], content)
                self.runtime_check(plan, expected)
        self.runtime_secrets()
        self.change_json("google_credentials_file", lambda value: value.update(private_key="PRIVATE_CANARY"))
        self.runtime_check(plan, "runtime_credentials_invalid")

    def test_runtime_secret_bodies_remain_unopened_on_unsafe_core_gateway_or_environment(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        child = self.start_gateway(plan)
        watch = self.watch_secret_inodes()
        self.runtime_check(plan, "launcher_process_identity_mismatch", clean=False)
        self.runtime_check(plan, "service_environment_conflict", env={"SECRETARY_CORE_TOKEN": "PRIVATE_CANARY"})
        self.runtime_check(plan, "service_environment_conflict", env={"https_proxy": "PRIVATE_CANARY"})
        child.stdin.write(b"x")
        child.stdin.flush()
        child.wait(timeout=3)
        self.runtime_check(plan, "gateway_process_unavailable")
        with self.assertRaises(BlockingIOError):
            os.read(watch, 65536)

    def runtime_barrier(self, plan, mode, change, expected, message="runtime_prepared"):
        child = subprocess.Popen(
            [*self.probe_arguments(self.manifest_path, mode=mode), str(plan)], env={},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            ready, _, _ = select.select([child.stdout], [], [], 10)
            self.assertTrue(ready, "runtime probe did not reach its barrier")
            self.assertEqual(child.stdout.readline(64), (message + "\n").encode())
            change()
            output, errors = child.communicate(b"x", timeout=10)
            self.assertEqual(child.returncode, int(expected != "passed"), errors)
            self.assertEqual(output, (expected + "\n").encode())
            self.assertEqual(errors, b"")
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=3)
            child.stdin.close()
            child.stdout.close()
            child.stderr.close()

    def test_runtime_revalidation_does_not_reopen_or_read_prepared_secret_bodies(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        watches = []
        def observe():
            # Start after the child has prepared and validated its borrowed inputs.
            # The watcher's control read is drained before releasing the child.
            watches.append(self.watch_secret_inodes())
        self.runtime_barrier(plan, "runtime-wait", observe, "passed")
        with self.assertRaises(BlockingIOError):
            os.read(watches[0], 65536)

    def test_runtime_lifetime_accepts_core_threads_tcp_and_private_journal_handles(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        result = subprocess.run([*self.probe_arguments(self.manifest_path, mode="runtime-live"), str(plan)], env={}, capture_output=True, timeout=10)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b"passed\n", b""))

    def test_runtime_lifetime_rejects_an_unregistered_core_file_and_remains_fenced(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        result = subprocess.run([*self.probe_arguments(self.manifest_path, mode="runtime-live-rogue"), str(plan)], env={}, capture_output=True, timeout=10)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b"passed\n", b""))

    def test_runtime_lifetime_does_not_accept_external_root_unix_channels(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        path=self.root / "external.sock"
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as server:
            server.bind(str(path));path.chmod(0o666);server.listen(1)
            result=subprocess.run([*self.probe_arguments(self.manifest_path,mode="runtime-live-external-unix"),str(plan),str(path)],env={},capture_output=True,timeout=10)
            self.assertEqual((result.returncode,result.stdout,result.stderr),(0,b"passed\n",b""))

    def provider(self):
        events=[]
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self,*args):
                pass
            def do_GET(self):
                events.append(("GET",self.path))
                if self.path.startswith('/v4/spreadsheets/generic-spreadsheet'):
                    value={"spreadsheetId":"generic-spreadsheet","sheets":[{"properties":{"sheetId":7,"title":"Operations"},"data":[{"startRow":1,"startColumn":1,"rowData":[{"values":[{"effectiveValue":{"stringValue":"é🙂"}},{"effectiveValue":{"numberValue":12}},{"effectiveValue":{"boolValue":True}}]}]}]}]}
                elif self.path.startswith('/v1/documents/demo-document'):
                    value={"documentId":"demo-document","title":"Fixture","revisionId":"r1","tabs":[{"tabProperties":{"tabId":"t.0","title":"Main"},"documentTab":{"body":{"content":[{"paragraph":{"elements":[{"textRun":{"content":"é🙂 sample\n"}}]}}]}}}]}
                else:
                    self.send_error(404);return
                body=json.dumps(value,ensure_ascii=False).encode()
                self.send_response(200);self.send_header('content-type','application/json');self.send_header('content-length',str(len(body)));self.end_headers();self.wfile.write(body)
            def do_POST(self):
                events.append(("MUTATION",self.path));self.send_error(405)
            do_PUT=do_POST
            do_PATCH=do_POST
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        worker=threading.Thread(target=server.serve_forever,daemon=True);worker.start()
        def cleanup():
            server.shutdown();server.server_close();worker.join(timeout=3)
        self.addCleanup(cleanup)
        return f'http://127.0.0.1:{server.server_port}',events

    def start_runtime_server(self,plan,provider):
        child=subprocess.Popen([*self.probe_arguments(self.manifest_path,mode='runtime-server'),str(plan),provider],env={},stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        def stop():
            if child.poll() is None:
                try:
                    child.stdin.write(b'x');child.stdin.flush();child.wait(timeout=3)
                except (BrokenPipeError,subprocess.TimeoutExpired):
                    child.kill();child.wait(timeout=3)
            for stream in (child.stdin,child.stdout,child.stderr):stream.close()
        self.addCleanup(stop)
        ready,_,_=select.select([child.stdout],[],[],10)
        self.assertTrue(ready,'runtime server did not reach ready barrier')
        line=child.stdout.readline(256)
        if not line.startswith(b'runtime_ready:'):
            child.wait(timeout=3)
            self.fail(f'Runtime fixture failed: {line!r}; {child.stderr.read()!r}')
        return 'http://'+line.decode().strip().split(':',1)[1],stop

    def runtime_http(self,url,path,value,operator=False,expected=200):
        token='SYNTHETIC-OPERATOR-9876543210-zyxw' if operator else 'SYNTHETIC-CORE-0123456789-abcdefgh'
        request=urllib.request.Request(url+path,json.dumps(value).encode(),headers={'content-type':'application/json','authorization':'Bearer '+token})
        try:
            response=urllib.request.urlopen(request,timeout=10)
        except urllib.error.HTTPError as error:
            response=error
        with response:
            self.assertEqual(response.status,expected)
            return json.loads(response.read())

    def read_task_limits(self,calls=4):
        return {'calls':calls,'rows':10,'document_characters':4096,'output_bytes':262144,'in_flight':1}

    def test_protected_runtime_builtin_sdk_http_and_real_adapter_obey_cumulative_budgets(self):
        self.runtime_secrets();plan,_=self.gateway_plan();self.start_gateway(plan)
        provider,events=self.provider();url,_=self.start_runtime_server(plan,provider)
        self.runtime_http(url,'/v1/read-budget/operator/create',self.read_task_limits(),operator=True)
        result=subprocess.run([shutil.which('node'),str(SOURCE/'scripts/acceptance-read-budget.mjs'),url,' exact synthetic owner '],env={'SECRETARY_CORE_TOKEN':'SYNTHETIC-CORE-0123456789-abcdefgh'},capture_output=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stderr)
        summary=json.loads(result.stdout)
        status=self.runtime_http(url,'/v1/read-budget/operator/status',{},operator=True)
        self.assertEqual(status['charged']['calls'],4)
        self.assertEqual(status['charged']['output_bytes'],summary['charged_bytes'])
        self.assertEqual(status['charged']['rows'],1)
        self.assertGreaterEqual(status['charged']['document_characters'],2)
        self.assertEqual(len(events),2)
        self.assertTrue(all(method=='GET' for method,_ in events))

    def test_protected_runtime_restart_cannot_reset_an_unfinished_read_task(self):
        self.runtime_secrets();plan,_=self.gateway_plan();self.start_gateway(plan)
        provider,events=self.provider();url,stop=self.start_runtime_server(plan,provider)
        task=self.runtime_http(url,'/v1/read-budget/operator/create',self.read_task_limits(),operator=True)
        admission=self.runtime_http(url,'/v1/read-budget/reserve',{'tool':'sheets_read'})
        self.assertIsNotNone(admission['grant']);stop()
        url,_=self.start_runtime_server(plan,provider)
        status=self.runtime_http(url,'/v1/read-budget/operator/status',{},operator=True)
        self.assertEqual(status['task_id'],task['task_id']);self.assertEqual(status['charged']['calls'],1)
        self.assertEqual(status['charged']['rows'],10);self.assertEqual(status['in_flight'],0)
        denied=self.runtime_http(url,'/v1/read-budget/reserve',{'tool':'sheets_read'})
        self.assertIsNone(denied['grant']);self.assertTrue(denied['model_text'])
        self.assertFalse(self.runtime_http(url,'/v1/read-budget/reserve',{'tool':'sheets_read'})['model_text'])
        self.assertEqual(events,[])

    def test_runtime_configuration_change_fences_future_budget_admission(self):
        self.runtime_secrets();plan,_=self.gateway_plan();self.start_gateway(plan)
        provider,events=self.provider();url,_=self.start_runtime_server(plan,provider)
        self.runtime_http(url,'/v1/read-budget/operator/create',self.read_task_limits(),operator=True)
        Path(self.manifest['core_token_file']).chmod(0o600)
        self.assertEqual(self.runtime_http(url,'/v1/read-budget/reserve',{'tool':'sheets_list'},expected=503)['error']['code'],'file_changed')
        Path(self.manifest['core_token_file']).chmod(0o640)
        self.assertEqual(self.runtime_http(url,'/v1/read-budget/reserve',{'tool':'sheets_list'},expected=503)['error']['code'],'runtime_fenced')
        self.assertEqual(events,[])

    def test_manifest_startup_stays_denied_before_journal_client_or_listener(self):
        self.runtime_secrets();plan,_=self.gateway_plan();self.start_gateway(plan)
        before=self.snapshot()
        command=[*self.probe_arguments(self.manifest_path)[:-2],self.binary,'service','start-readonly','--manifest',str(self.manifest_path),'--gateway-plan',str(plan)]
        result=subprocess.run(command,env={},capture_output=True,timeout=10)
        self.assertEqual((result.returncode,result.stdout,result.stderr),(1,b'',b''))
        self.assertEqual(self.snapshot(),before)

    def test_runtime_rejects_secret_authority_change_after_static_snapshot_without_reading_body(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        watch = self.watch_secret_inodes()
        self.runtime_barrier(plan, "runtime-before-wait",
                             lambda: Path(self.manifest["core_token_file"]).chmod(0o600),
                             "file_changed", "runtime_snapshot_loaded")
        with self.assertRaises(BlockingIOError):
            os.read(watch, 65536)

    def test_runtime_rejects_an_inherited_private_handle_before_reading_secrets(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        private = self.root / "inherited"
        private.write_bytes(b"SYNTHETIC_INHERITED_PRIVATE_BODY")
        private.chmod(0o600)
        watch = self.watch_secret_inodes()
        with private.open("rb") as inherited:
            result = subprocess.run(
                [*self.probe_arguments(self.manifest_path, mode="runtime"), str(plan)],
                env={}, pass_fds=(inherited.fileno(),), capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"launcher_inherited_descriptor\n")
        self.assertEqual(result.stderr, b"")
        with self.assertRaises(BlockingIOError):
            os.read(watch, 65536)

    def test_runtime_denial_does_not_print_into_unsafe_standard_output(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        self.start_gateway(plan)
        output = self.root / "unsafe-stdout"
        output.write_bytes(b"SYNTHETIC_STDIO_SENTINEL")
        watch = self.watch_secret_inodes()
        with output.open("ab") as file:
            result = subprocess.run(
                [*self.probe_arguments(self.manifest_path, mode="runtime-quiet"), str(plan)],
                env={}, stdout=file, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, timeout=10)
        self.assertEqual(result.returncode, 42)
        self.assertEqual(result.stderr, b"")
        self.assertEqual(output.read_bytes(), b"SYNTHETIC_STDIO_SENTINEL")
        with self.assertRaises(BlockingIOError):
            os.read(watch, 65536)

    def test_runtime_rechecks_all_pinned_inputs_before_returning_borrowed_data(self):
        self.runtime_secrets()
        plan, value = self.gateway_plan()
        self.start_gateway(plan)
        for path in (self.manifest["core_token_file"], self.manifest["registry_file"],
                     str(plan), value["intent_file"], value["executable"]):
            with self.subTest(input=Path(path).name):
                original_mode = stat.S_IMODE(Path(path).stat().st_mode)
                try:
                    # chmod also changes ctime without a body rewrite or any
                    # assumption about timestamp granularity on identical writes.
                    self.runtime_barrier(plan, "runtime-wait", lambda: Path(path).chmod(0o600), "file_changed")
                finally:
                    # Only permissions changed. Rewriting the live ELF would
                    # fail with ETXTBSY and obscure the authority assertion.
                    Path(path).chmod(original_mode)

    def test_runtime_rechecks_gateway_liveness_before_returning_borrowed_data(self):
        self.runtime_secrets()
        plan, _ = self.gateway_plan()
        child = self.start_gateway(plan)
        def stop():
            child.stdin.write(b"x")
            child.stdin.flush()
            child.wait(timeout=3)
        self.runtime_barrier(plan, "runtime-wait", stop, "gateway_process_unavailable")

    def test_gateway_intent_cannot_be_replaced_or_replayed_to_another_process(self):
        plan, value = self.gateway_plan()
        self.start_gateway(plan)
        intent = Path(value["intent_file"])
        before = intent.read_bytes(), intent.stat().st_ino
        result = subprocess.run(self.gateway_arguments(plan), env={}, capture_output=True, timeout=5)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        self.assertEqual((intent.read_bytes(), intent.stat().st_ino), before)
        self.check_gateway(plan)

    def test_gateway_dead_process_and_stale_pid_intent_are_denied(self):
        plan, _ = self.gateway_plan()
        child = self.start_gateway(plan)
        child.stdin.write(b"x")
        child.stdin.flush()
        child.wait(timeout=3)
        self.check_gateway(plan, "gateway_process_unavailable")

    def test_gateway_start_ticks_and_boot_binding_are_enforced(self):
        plan, value = self.gateway_plan()
        self.start_gateway(plan)
        path = Path(value["intent_file"])
        original = json.loads(path.read_bytes())
        for field, replacement, error in [("start_ticks", original["start_ticks"] + 1, "gateway_process_changed"),
                                          ("boot_id", "00000000-0000-0000-0000-000000000000", "gateway_intent_binding"),
                                          ("plan_sha256", "f" * 64, "gateway_intent_binding")]:
            with self.subTest(field=field):
                self.write_json(path, {**original, field: replacement}, 0o440)
                self.check_gateway(plan, error)
        self.write_json(path, original, 0o440)
        self.check_gateway(plan)

    def test_gateway_namespace_and_mapping_mismatch_are_denied(self):
        plan, value = self.gateway_plan()
        self.start_gateway(plan)
        path = Path(value["intent_file"])
        original = json.loads(path.read_bytes())
        for field in ("mnt", "user", "pid", "net"):
            with self.subTest(namespace=field):
                changed = json.loads(json.dumps(original))
                changed["context"]["namespaces"][field]["inode"] += 1
                self.write_json(path, changed, 0o440)
                self.check_gateway(plan, "gateway_namespace_mismatch")
        changed = json.loads(json.dumps(original))
        changed["context"]["uid_map"] = [[0, 0, 1]]
        self.write_json(path, changed, 0o440)
        self.check_gateway(plan, "gateway_namespace_mismatch")

    def test_gateway_root_owned_intent_cannot_be_forged_by_service_identity(self):
        plan, value = self.gateway_plan()
        self.start_gateway(plan)
        intent = Path(value["intent_file"])
        os.chown(intent, 1, 1)
        self.check_gateway(plan, "unsafe_input")

    def test_gateway_current_kernel_authority_is_required_even_with_bound_root_intent(self):
        plan, value = self.gateway_plan()
        self.start_gateway(plan)
        path = Path(value["intent_file"])
        original = json.loads(path.read_bytes())
        cases = [({}, "launcher_process_identity_mismatch"),
                 ({"user": 1, "group": 1, "extra_groups": []}, "launcher_process_identity_mismatch"),
                 ({"user": 65534, "group": 65534, "extra_groups": [1]}, "launcher_supplementary_groups"),
                 ({"user": 65534, "group": 65534, "extra_groups": []}, "launcher_capabilities")]
        for identity, error in cases:
            with self.subTest(expected=error, identity=identity):
                with subprocess.Popen(["/usr/bin/cat"], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                      env={}, **identity) as child:
                    try:
                        # The test administrator can publish an intent. It still
                        # must not substitute for the actual process identity.
                        fields = Path(f"/proc/{child.pid}/stat").read_text().rsplit(") ", 1)[1].split()
                        self.write_json(path, {**original, "pid": child.pid,
                                               "start_ticks": int(fields[19])}, 0o440)
                        self.check_gateway(plan, error)
                    finally:
                        # No CAP_KILL is granted to the test container. EOF lets
                        # the differently owned child exit without extra privilege.
                        child.stdin.close()
                        child.wait(timeout=3)

    def test_gateway_failed_exec_leaves_a_non_admitting_intent(self):
        plan, value = self.gateway_plan()
        # Hash-valid ELF magic is not proof the kernel can execute this image.
        self.write(value["executable"], b"\x7fELFsynthetic-invalid-image", 0o755)
        value["executable_sha256"] = hashlib.sha256(Path(value["executable"]).read_bytes()).hexdigest()
        self.write_json(plan, value)
        result = subprocess.run(self.gateway_arguments(plan), env={}, capture_output=True, timeout=5)
        self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        self.assertTrue(Path(value["intent_file"]).is_file())
        self.check_gateway(plan, "gateway_process_unavailable")

    def test_gateway_plan_is_bound_to_manifest_and_executable(self):
        plan, value = self.gateway_plan()
        for field in ("manifest_sha256", "policy_sha256", "executable_sha256"):
            with self.subTest(field=field):
                self.write_json(plan, {**value, field: "f" * 64})
                result = subprocess.run(self.gateway_arguments(plan), env={}, capture_output=True, timeout=5)
                self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
                self.assertFalse(Path(value["intent_file"]).exists())

    def test_gateway_exec_rejects_nonroot_unknown_handles_and_unsafe_stdio(self):
        plan, value = self.gateway_plan()
        result = subprocess.run(self.gateway_arguments(plan), env={}, capture_output=True, timeout=5,
                                user=1, group=1, extra_groups=[])
        self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        private = self.inherited_private_file()
        with private.open("rb") as file:
            result = subprocess.run(self.gateway_arguments(plan), env={}, capture_output=True, timeout=5,
                                    pass_fds=(file.fileno(),))
        self.assertEqual((result.returncode, result.stdout, result.stderr), (1, b"", b""))
        original = private.read_bytes()
        with private.open("r+b") as file:
            result = subprocess.run(self.gateway_arguments(plan), env={}, stdout=file,
                                    stderr=subprocess.PIPE, timeout=5)
        self.assertEqual((result.returncode, result.stderr), (1, b""))
        self.assertEqual(private.read_bytes(), original)
        self.assertFalse(Path(value["intent_file"]).exists())

    def test_launcher_accepts_exact_non_root_authority_with_no_extra_groups(self):
        self.launcher()

    def test_launcher_accepts_a_repeated_primary_group(self):
        self.launcher(groups="1")

    def test_launcher_rejects_a_gateway_supplementary_group(self):
        self.launcher("launcher_supplementary_groups", groups="65534")

    def test_launcher_rejects_retained_capability_bounding_set(self):
        self.launcher("launcher_capabilities", clean=False, as_core=True)

    def test_launcher_rejects_root_even_on_safe_static_files(self):
        self.launcher("launcher_process_identity_mismatch", clean=False)

    def test_launcher_rejects_saved_root_with_non_root_effective_identity(self):
        self.launcher("launcher_process_identity_mismatch", clean=False, mode="saved-root")

    def test_launcher_rejects_starting_workers_before_the_authority_check(self):
        self.launcher("launcher_thread_state", mode="threaded")

    def inherited_private_file(self):
        path = self.root / "inherited-private"
        path.write_bytes(b"SYNTHETIC_INHERITED_PRIVATE_BODY")
        os.chown(path, 0, 0)
        path.chmod(0o600)
        return path

    def test_inherited_root_read_handle_is_denied_without_opening_or_reading_its_body(self):
        path = self.inherited_private_file()
        descriptor = os.open(path, os.O_RDONLY)
        self.addCleanup(os.close, descriptor)
        arguments = self.probe_arguments(self.manifest["service_policy_file"],
                                         mode="descriptor-read-control")
        result = subprocess.run([*arguments, str(descriptor)], pass_fds=(descriptor,),
                                env={}, capture_output=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b"control_read\n")
        os.lseek(descriptor, 0, os.SEEK_SET)
        watched = self.watch_secret_inodes([path])
        result = subprocess.run(self.probe_arguments(self.manifest["service_policy_file"]),
                                pass_fds=(descriptor,), env={}, capture_output=True,
                                timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"launcher_inherited_descriptor\n")
        self.assertEqual(result.stderr, b"")
        with self.assertRaises(BlockingIOError):
            os.read(watched, 65536)

    def test_inherited_root_write_handle_is_denied_without_modifying_the_file(self):
        path = self.inherited_private_file()
        descriptor = os.open(path, os.O_WRONLY)
        self.addCleanup(os.close, descriptor)
        result = subprocess.run(self.probe_arguments(self.manifest["service_policy_file"]),
                                pass_fds=(descriptor,), env={}, capture_output=True,
                                timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"launcher_inherited_descriptor\n")
        self.assertEqual(path.read_bytes(), b"SYNTHETIC_INHERITED_PRIVATE_BODY")

    def test_connected_root_unix_socket_is_denied_as_extra_fd_and_stdin(self):
        local, peer = socket.socketpair()
        with local, peer:
            credentials = struct.unpack("3i", local.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            self.assertEqual(credentials[1], 0)
            result = subprocess.run(self.probe_arguments(self.manifest["service_policy_file"]),
                                    pass_fds=(local.fileno(),), env={}, capture_output=True,
                                    timeout=5, check=False)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, b"launcher_inherited_descriptor\n")
            result = subprocess.run(self.probe_arguments(self.manifest["service_policy_file"],
                                                         mode="descriptor-quiet"),
                                    stdin=local, env={}, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, timeout=5, check=False)
            self.assertEqual(result.returncode, 42)
            self.assertEqual(result.stdout + result.stderr, b"")
            peer.setblocking(False)
            with self.assertRaises(BlockingIOError):
                peer.recv(1)

    def test_regular_standard_io_is_denied_without_printing_into_it(self):
        path = self.inherited_private_file()
        for stream in ("stdin", "stdout", "stderr"):
            with self.subTest(stream=stream), path.open("r+b") as handle:
                streams = {"stdout": subprocess.PIPE, "stderr": subprocess.PIPE, stream: handle}
                result = subprocess.run(self.probe_arguments(self.manifest["service_policy_file"],
                                                             mode="descriptor-quiet"),
                                        env={}, timeout=5, check=False, **streams)
                self.assertEqual(result.returncode, 42)
                self.assertEqual(path.read_bytes(), b"SYNTHETIC_INHERITED_PRIVATE_BODY")

    def test_named_fifo_stdin_is_not_mistaken_for_an_anonymous_pipe(self):
        path = self.root / "named-channel"
        os.mkfifo(path, 0o600)
        keepalive = os.open(path, os.O_RDWR | os.O_NONBLOCK)
        self.addCleanup(os.close, keepalive)
        reader = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
        self.addCleanup(os.close, reader)
        result = subprocess.run(self.probe_arguments(self.manifest["service_policy_file"],
                                                     mode="descriptor-quiet"),
                                stdin=reader, env={}, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=5, check=False)
        self.assertEqual(result.returncode, 42)
        self.assertEqual(result.stdout + result.stderr, b"")

    def test_unlisted_extra_anonymous_pipe_is_denied(self):
        reader, writer = os.pipe()
        self.addCleanup(os.close, reader)
        self.addCleanup(os.close, writer)
        result = subprocess.run(self.probe_arguments(self.manifest["service_policy_file"]),
                                pass_fds=(reader,), env={}, capture_output=True,
                                timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"launcher_inherited_descriptor\n")

    def test_snapshot_allowlist_cannot_admit_an_unrelated_inherited_handle(self):
        path = self.inherited_private_file()
        descriptor = os.open(path, os.O_RDONLY)
        self.addCleanup(os.close, descriptor)
        result = subprocess.run(self.probe_arguments(self.manifest_path, mode="snapshot"),
                                pass_fds=(descriptor,), env={}, capture_output=True,
                                timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"launcher_inherited_descriptor\n")

    def test_snapshot_held_input_must_still_be_close_on_exec(self):
        result = subprocess.run(self.probe_arguments(self.manifest_path, mode="snapshot-no-cloexec"),
                                env={}, capture_output=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"launcher_inherited_descriptor\n")

    def test_protected_snapshot_binds_non_root_process_to_one_validated_configuration(self):
        result = subprocess.run(self.probe_arguments(self.manifest_path, mode="snapshot"),
                                env={}, capture_output=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b"passed\n")
        self.assertEqual(result.stderr, b"")

    def test_protected_snapshot_does_not_grant_root_runtime_authority(self):
        result = subprocess.run(self.probe_arguments(self.manifest_path, mode="snapshot", clean=False),
                                env={}, capture_output=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"launcher_process_identity_mismatch\n")
        self.assertEqual(result.stderr, b"")

    def changed_snapshot(self, path, mode):
        path = Path(path)
        original = path.read_bytes()
        child = subprocess.Popen(self.probe_arguments(self.manifest_path, mode=mode),
                                 env={}, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE)
        try:
            ready, _, _ = select.select([child.stdout], [], [], 5)
            self.assertTrue(ready, "snapshot probe failed to reach the loaded barrier")
            self.assertEqual(child.stdout.readline(64), b"snapshot_loaded\n")
            # Only the administrator-side test parent changes the protected file.
            # The non-root child cannot silently adopt the replacement bytes.
            path.write_bytes(b"{}")
            output, errors = child.communicate(b"1", timeout=5)
            self.assertEqual(child.returncode, 1, errors)
            self.assertEqual(output, b"file_changed\n")
            self.assertEqual(errors, b"")
        finally:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=5)
            for stream in (child.stdin, child.stdout, child.stderr):
                stream.close()
            path.write_bytes(original)

    def test_protected_snapshot_rejects_changed_manifest_policy_registry_secret_and_artifact(self):
        for path in (self.manifest_path, self.manifest["service_policy_file"],
                     self.manifest["registry_file"], self.manifest["core_token_file"],
                     self.root / "release/bin/secretary-core"):
            with self.subTest(input=Path(path).name):
                self.changed_snapshot(path, "snapshot-wait")

    def test_protected_snapshot_accessors_cannot_return_stale_configuration(self):
        for mode in ("snapshot-registry-wait", "snapshot-generation-wait", "snapshot-report-wait"):
            with self.subTest(mode=mode):
                self.changed_snapshot(self.manifest["registry_file"], mode)

    def watch_secret_inodes(self, paths=None):
        libc = ctypes.CDLL(None, use_errno=True)
        libc.inotify_init1.argtypes = [ctypes.c_int]
        libc.inotify_add_watch.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_uint32]
        descriptor = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
        self.assertGreaterEqual(descriptor, 0)
        self.addCleanup(os.close, descriptor)
        # IN_OPEN | IN_ACCESS observe readable opens/reads; O_PATH metadata handles
        # do not emit them. Prove the watcher works before trusting an empty queue.
        paths = paths or [self.manifest[field] for field in SECRET_FIELDS]
        for path in paths:
            self.assertGreaterEqual(libc.inotify_add_watch(
                descriptor, os.fsencode(path), 0x20 | 0x01), 0)
        Path(paths[0]).read_bytes()
        self.assertTrue(os.read(descriptor, 65536), "inotify did not observe control read")
        return descriptor

    def test_protected_snapshot_never_opens_or_reads_secret_bodies(self):
        descriptor = self.watch_secret_inodes()
        result = subprocess.run(self.probe_arguments(self.manifest_path, mode="snapshot"),
                                env={}, capture_output=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b"passed\n")
        self.assertEqual(result.stderr, b"")
        with self.assertRaises(BlockingIOError):
            os.read(descriptor, 65536)

    def test_secret_inodes_have_no_open_or_access_events(self):
        descriptor = self.watch_secret_inodes()
        # Do not use check() here: its harness snapshots deliberately read bodies.
        result = subprocess.run(
            [self.binary, "service", "check", "--manifest", str(self.manifest_path)],
            env={}, capture_output=True, timeout=5, check=False,
            user=1, group=1, extra_groups=[],
        )
        self.assertEqual(result.returncode, 3, result.stdout)
        self.assertEqual(json.loads(result.stdout)["blockers"], BLOCKERS)
        with self.assertRaises(BlockingIOError):
            os.read(descriptor, 65536)

    def test_final_symlink_is_denied(self):
        path = Path(self.manifest["owner_file"])
        path.unlink()
        path.symlink_to(self.manifest["core_token_file"])
        self.check("unsafe_input")

    def test_ancestor_symlink_is_denied(self):
        link = self.root / "linked"
        link.symlink_to(self.config, target_is_directory=True)
        self.manifest_path = link / "manifest.json"
        self.check("file_unavailable")

    def test_hard_link_is_denied(self):
        os.link(self.manifest["owner_file"], self.config / "alias")
        self.check("unsafe_input")

    def test_fifo_is_denied_without_blocking(self):
        path = Path(self.manifest["owner_file"])
        path.unlink()
        os.mkfifo(path, 0o640)
        os.chown(path, 0, 1)
        self.check("unsafe_input")

    def test_device_is_denied(self):
        # A device under protected ancestors reaches the final inode guard;
        # /dev itself may instead fail the filesystem's ACL-support check.
        path = Path(self.manifest["owner_file"])
        path.unlink()
        os.mknod(path, stat.S_IFCHR | 0o640, os.makedev(1, 3))
        os.chown(path, 0, 1)
        self.check("unsafe_input")

    def test_writable_ancestor_is_denied(self):
        self.config.chmod(0o775)
        self.check("unsafe_ancestor")

    def test_non_root_ancestor_is_denied(self):
        os.chown(self.config, 1, 1)
        self.check("unsafe_ancestor")

    def test_file_access_acl_is_denied(self):
        os.setxattr(self.manifest["owner_file"], "system.posix_acl_access", acl_bytes())
        self.check("extended_acl_forbidden")

    def test_ancestor_default_acl_is_denied(self):
        os.setxattr(self.config, "system.posix_acl_default", acl_bytes())
        self.check("extended_acl_forbidden")

    def test_private_file_mode_is_denied(self):
        Path(self.manifest["google_credentials_file"]).chmod(0o644)
        self.check("private_file_policy")

    def test_private_file_owner_is_denied(self):
        os.chown(self.manifest["core_token_file"], 1, 1)
        self.check("unsafe_input")

    def test_private_file_group_is_denied(self):
        os.chown(self.manifest["core_token_file"], 0, 65534)
        self.check("private_file_policy")

    def test_state_mode_is_denied(self):
        self.state.chmod(0o750)
        self.check("private_state_policy")

    def test_state_owner_is_denied(self):
        os.chown(self.state, 65534, 1)
        self.check("private_state_policy")

    def test_core_cannot_traverse_private_administrator_ancestor(self):
        self.config.chmod(0o700)
        self.check("service_directory_access")

    def test_duplicate_decoded_key_is_denied(self):
        raw = self.manifest_path.read_bytes()
        self.write(self.manifest_path, b'{"\\u0073chema":"canary",' + raw[1:])
        self.check("configuration_json_invalid")

    def test_dormant_write_grant_is_denied(self):
        self.change_json("registry_file", lambda value: value["documents"][0].update(
            readable=False, writable=True))
        self.check("registry_readonly_invalid")

    def test_release_hash_mismatch_is_denied(self):
        self.write(self.root / "release/bin/secretary-core", b"tampered", 0o755)
        self.check("release_hash_mismatch")

    def test_release_missing_executable_bit_is_denied(self):
        (self.root / "release/bin/secretary-core").chmod(0o644)
        self.check("release_not_executable")

    def test_missing_local_identity_is_denied(self):
        self.change_json("service_policy_file", lambda value: value.update(
            gateway={"uid": 4000000000, "gid": 4000000000}))
        self.check("local_identity_mismatch")

    def test_environment_override_is_denied_on_valid_fixture(self):
        self.check("service_environment_conflict", {"https_proxy": "SYNTHETIC_ENV_CANARY"})


if __name__ == "__main__":
    if sys.platform != "linux" or os.geteuid() != 0 or not Path("/.dockerenv").is_file():
        sys.exit("Run the acceptance launcher in a disposable Linux container as root.")
    if len(sys.argv) != 2:
        sys.exit("Expected the compiled native secretary-core binary path.")
    ServiceCheckLinux.binary = str(Path(sys.argv.pop()).resolve(strict=True))
    unittest.main(verbosity=2)
