// Tests for the release scripts: node --test scripts/release/
import assert from "node:assert/strict";
import { test } from "node:test";
import { notesFor } from "./notes.mjs";
import { lockFile, msiVersion, normalize } from "./version.mjs";

test("any common way of writing a version becomes major.minor.patch", () => {
  const cases = {
    "0.2.0": "0.2.0",
    "v0.1.25": "0.1.25",
    "V1.2.3": "1.2.3",
    " 1.0 ": "1.0.0",
    2: "2.0.0",
    "01.02.03": "1.2.3",
    "0.3.0-beta.1": "0.3.0-beta.1",
    "v1.0-rc.02": "1.0.0-rc.2",
    "10.20.300": "10.20.300",
  };
  for (const [input, want] of Object.entries(cases)) assert.equal(normalize(input), want, input);
});

test("what is not a version is refused with a clear message", () => {
  for (const bad of ["", "abc", "1.2.3.4", "1.2.3+build", "1..2", "v", "1.2.3-"]) {
    assert.throws(() => normalize(bad), /not a version/, bad);
  }
});

test("the Windows installer gets numbers only, within its limits", () => {
  assert.equal(msiVersion("0.2.0"), "0.2.0");
  assert.equal(msiVersion("v1.4"), "1.4.0");
  assert.equal(msiVersion("0.3.0-beta.1"), "0.3.0");
  assert.equal(msiVersion("255.255.65535"), "255.255.65535");
  assert.throws(() => msiVersion("256.0.0"), /Windows installer/);
  assert.throws(() => msiVersion("1.0.65536"), /Windows installer/);
});

const CHANGELOG = `# Changelog

## Unreleased

- Next thing.

## 0.1.25 — 2026-11-09

- Twenty-five.

## [v0.1.2]

- Two.

## 0.1.0

- First.
`;

test("release notes come from the version's own section, matched exactly", () => {
  assert.deepEqual(notesFor(CHANGELOG, "0.1.25"), { text: "- Twenty-five.", from: "0.1.25" });
  // 0.1.2 must not pick up the 0.1.25 section above it.
  assert.deepEqual(notesFor(CHANGELOG, "v0.1.2"), { text: "- Two.", from: "0.1.2" });
  assert.deepEqual(notesFor(CHANGELOG, "0.1"), { text: "- First.", from: "0.1.0" });
});

test("without a section the notes are the Unreleased ones, then a plain line", () => {
  assert.deepEqual(notesFor(CHANGELOG, "0.2.0"), { text: "- Next thing.", from: "Unreleased" });
  assert.deepEqual(notesFor("# Changelog\n\n## Unreleased\n\n## 0.1.0\n\n- x\n", "0.2.0"), {
    text: "Polyph 0.2.0.",
    from: null,
  });
});

const LOCK = `version = 4

[[package]]
name = "anyhow"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "polyph-cli"
version = "0.1.2"
dependencies = [
 "anyhow",
]

[[package]]
name = "polyph-core"
version = "0.1.2"
`;

test("the workspace's own crates in a lock file get the version, on either line ending", () => {
  for (const eol of ["\n", "\r\n"]) {
    const text = LOCK.replace(/\n/g, eol);
    const l = lockFile("Cargo.lock");
    assert.equal(l.get(text), "0.1.2");
    const out = l.set(text, "0.1.3");
    assert.equal(l.get(out), "0.1.3");
    assert.match(out, /name = "anyhow"\r?\nversion = "1.0.0"/, "registry crates stay");
    assert.equal(out.split("\r\n").length > 1, eol === "\r\n", "line endings kept");
    assert.equal(out.replace(/0\.1\.3/g, "0.1.2"), text, "nothing else changes");
  }
});
