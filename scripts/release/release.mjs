#!/usr/bin/env node
// The release workflow's helper (ADR 0098). Every command is a thin wrapper over release-lib.mjs.
//   node scripts/release/release.mjs check-tag <tag>          the tag matches every version file
//   node scripts/release/release.mjs tags <tag> <repository>  image refs, comma-separated
//   node scripts/release/release.mjs ci-gate <owner/repo> <sha> [timeout-minutes]
//   node scripts/release/release.mjs render-compose <version> <digests.json> <out>
//   node scripts/release/release.mjs notes <tag> <owner/repo> <body-in> <body-out>
//   node scripts/release/release.mjs fetch-stockfish-source <dir>
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ciGateVerdict,
  composeImageRefs,
  imageTags,
  mergeReleaseNotes,
  parseSourceManifest,
  parseTag,
  renderReleaseCompose,
  renderTemplate,
  requiredCiChecks,
} from "./release-lib.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (p) => readFileSync(join(root, p), "utf8").replaceAll("\r\n", "\n");
const die = (msg) => {
  console.error(`::error::${msg}`);
  process.exit(1);
};

const [command, ...args] = process.argv.slice(2);

async function github(path) {
  const token = process.env.GITHUB_TOKEN;
  const res = await fetch(`https://api.github.com/${path}`, {
    headers: {
      accept: "application/vnd.github+json",
      "x-github-api-version": "2022-11-28",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
  });
  if (!res.ok) throw new Error(`GET ${path}: HTTP ${res.status} ${await res.text()}`);
  return res.json();
}

async function checkRuns(repo, sha) {
  const runs = [];
  for (let page = 1; ; page += 1) {
    const body = await github(`repos/${repo}/commits/${sha}/check-runs?per_page=100&page=${page}`);
    runs.push(...body.check_runs);
    if (body.check_runs.length < 100) return runs;
  }
}

switch (command) {
  case "check-tag": {
    const t = parseTag(args[0]);
    const version = JSON.parse(read("package.json")).version;
    if (t.version !== version) die(`tag ${args[0]} but package.json is ${version}`);
    for (const r of composeImageRefs(read("deploy/compose.yaml"))) {
      if (r.version !== version) die(`deploy/compose.yaml: rematch-${r.image} is ${r.version}`);
    }
    console.log(t.version);
    break;
  }
  case "tags": {
    const [tag, repository] = args;
    if (!repository) die("usage: tags <tag> <repository>");
    console.log(
      imageTags(tag)
        .map((t) => `${repository}:${t}`)
        .join(","),
    );
    break;
  }
  case "ci-gate": {
    // Waits for the tagged commit's CI run on main (the push that merged the release PR) and
    // passes only when every required check succeeded on that exact commit.
    const [repo, sha, minutes = "120"] = args;
    if (!repo || !/^[0-9a-f]{40}$/.test(sha ?? "")) die("usage: ci-gate <owner/repo> <sha> [min]");
    const required = requiredCiChecks(
      JSON.parse(read(".github/settings/rulesets/main.json")),
      read(".github/workflows/ci.yml"),
    );
    console.log(`waiting for ${required.join(", ")} on ${sha}`);
    const deadline = Date.now() + Number(minutes) * 60_000;
    for (;;) {
      const verdict = ciGateVerdict(required, await checkRuns(repo, sha));
      console.log(`${new Date().toISOString()} ${verdict.state}: ${verdict.detail}`);
      if (verdict.state === "pass") break;
      if (verdict.state === "fail") {
        die(
          `CI did not pass on ${sha}: ${verdict.detail}. Re-run the failed CI jobs on that commit, then re-run this job.`,
        );
      }
      if (Date.now() > deadline) die(`CI still running on ${sha} after ${minutes} minutes`);
      await new Promise((r) => setTimeout(r, 60_000));
    }
    break;
  }
  case "render-compose": {
    const [version, digestsFile, out] = args;
    const digests = JSON.parse(readFileSync(digestsFile, "utf8"));
    writeFileSync(out, renderReleaseCompose(read("deploy/compose.yaml"), { version, digests }));
    break;
  }
  case "notes": {
    const [tag, repo, bodyIn, bodyOut] = args;
    const t = parseTag(tag);
    const header = renderTemplate(read(".github/release-notes/header.md"), {
      version: t.version,
      tag,
      repo,
      status: t.major === 0 ? "beta" : t.pre ? "prerelease" : "stable",
    });
    writeFileSync(bodyOut, mergeReleaseNotes(readFileSync(bodyIn, "utf8"), header));
    break;
  }
  case "fetch-stockfish-source": {
    const [dir] = args;
    for (const e of parseSourceManifest(read("deploy/stockfish-source.txt"))) {
      const res = await fetch(e.url, { redirect: "follow" });
      if (!res.ok) die(`${e.url}: HTTP ${res.status}`);
      const bytes = Buffer.from(await res.arrayBuffer());
      const got = createHash("sha256").update(bytes).digest("hex");
      if (got !== e.sha256) {
        die(`${e.file}: SHA-256 ${got}, deploy/stockfish-source.txt pins ${e.sha256} (${e.url})`);
      }
      writeFileSync(join(dir, e.file), bytes);
      console.log(`ok   ${e.file} ${got}`);
    }
    break;
  }
  default:
    die(`unknown command "${command}" (see the header of scripts/release/release.mjs)`);
}
