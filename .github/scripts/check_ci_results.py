#!/usr/bin/env python3

"""Fail a terminal CI job unless every serialized dependency succeeded.

Parent workflows pass GitHub's `toJSON(needs)` object through the NEEDS
environment variable. Treat skipped and cancelled dependencies as failures too:
for a required fan-in job, only an explicit success is safe to accept.

Fork exception: some upstream checks depend on infrastructure that is not
available in this fork (repo-name self-hosted runner groups, the BuildBuddy
Bazel remote-cache secret, and the `bazel` deployment environment). Those jobs
cannot pass here and are not part of the Cargo-only merge gate, so they are
listed in OPTIONAL_DEPENDENCIES and never block the required check. Every other
dependency must still succeed.
"""

import json
import os


# Dependency names (keys of the `needs` object) that are allowed to be
# non-success on this fork because they require infrastructure the fork does
# not have. Keep this list minimal and documented.
OPTIONAL_DEPENDENCIES = {
    # Bazel build/test/clippy + release verification. The fork gates on Cargo,
    # not Bazel, and lacks the BuildBuddy remote-cache secret.
    "bazel",
    # Python/TypeScript SDK tests run on repo-name self-hosted runners and a
    # glibc Docker image that the fork does not provision.
    "sdk",
}


def main() -> None:
    # Keep result policy in one script so blocking-ci and postmerge-ci cannot
    # drift in how they interpret dependency conclusions.
    needs = json.loads(os.environ["NEEDS"])

    skipped_optional = sorted(
        (name, dependency["result"])
        for name, dependency in needs.items()
        if name in OPTIONAL_DEPENDENCIES and dependency["result"] != "success"
    )
    failures = sorted(
        (name, dependency["result"])
        for name, dependency in needs.items()
        if name not in OPTIONAL_DEPENDENCIES and dependency["result"] != "success"
    )

    if skipped_optional:
        print("Ignoring non-success optional CI dependencies (fork infrastructure):")
        for name, result in skipped_optional:
            print(f"{name}: {result}")

    if failures:
        print("CI dependencies did not succeed:")
        for name, result in failures:
            print(f"{name}: {result}")
        raise SystemExit(1)

    print("All required CI dependencies succeeded.")


if __name__ == "__main__":
    main()
