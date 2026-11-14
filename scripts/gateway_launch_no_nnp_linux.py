"""Prove the real launcher sets NoNewPrivileges rather than merely inheriting it.

Run after native acceptance in a disposable root Linux container, using the same
synthetic fixture capabilities and compiled target, without Docker's
no-new-privileges security option. Core probes separately arrange their own NNP;
the Gateway launcher under test starts with the parent's observed NNP=0.
"""

import os
from pathlib import Path
import sys
import unittest

from service_check_linux import ServiceCheckLinux


class GatewayExecWithoutInheritedNnp(ServiceCheckLinux):
    def probe_arguments(self, *args, **kwargs):
        # This configures only the independent Core checker. The real root
        # gateway-exec command uses gateway_arguments and receives no such wrapper.
        return ["/usr/bin/setpriv", "--no-new-privs",
                *super().probe_arguments(*args, **kwargs)]


if __name__ == "__main__":
    if sys.platform != "linux" or os.geteuid() != 0 or not Path("/.dockerenv").is_file():
        sys.exit("Run in a disposable Linux container as root.")
    if len(sys.argv) != 2:
        sys.exit("Expected the compiled native secretary-core binary path.")
    if "NoNewPrivs:\t0" not in Path("/proc/self/status").read_text().splitlines():
        sys.exit("This fixture requires observed inherited NoNewPrivileges=0.")
    GatewayExecWithoutInheritedNnp.binary = str(Path(sys.argv[1]).resolve(strict=True))
    tests = unittest.TestSuite(GatewayExecWithoutInheritedNnp(name) for name in (
        "test_gateway_exec_drops_authority_private_access_and_inherited_environment",
        "test_gateway_exec_allows_runtime_threads_only_after_guarded_exec",
    ))
    result = unittest.TextTestRunner(verbosity=2).run(tests)
    sys.exit(not result.wasSuccessful())
