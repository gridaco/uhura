#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = dirname(fileURLToPath(import.meta.url));
const PROTOCOL_PATH = join(ROOT, "protocol.json");
const MULTI_CHARACTER_TOKENS = [
  "!==",
  "===",
  "...",
  "=>",
  "->",
  "::",
  "==",
  "!=",
  "<=",
  ">=",
  "??",
  "&&",
  "||",
  "+=",
  "-=",
  "*=",
  "/=",
];

function fail(message) {
  throw new Error(message);
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    fail(`${path}: ${error.message}`);
  }
}

function readProtocol() {
  return readJson(PROTOCOL_PATH);
}

function resolveFromRoot(path) {
  return resolve(ROOT, path);
}

function assertRegularFile(path) {
  if (!existsSync(path)) fail(`missing file: ${path}`);
  const stat = lstatSync(path);
  if (stat.isSymbolicLink() || !stat.isFile()) {
    fail(`expected a regular non-symlink file: ${path}`);
  }
}

function assertDirectory(path) {
  if (!existsSync(path) || !lstatSync(path).isDirectory()) {
    fail(`expected directory: ${path}`);
  }
}

function unique(values, label) {
  const seen = new Set();
  for (const value of values) {
    if (seen.has(value)) fail(`duplicate ${label}: ${value}`);
    seen.add(value);
  }
}

function lex(source) {
  let index = 0;
  let line = 1;
  const tokens = [];
  const codeLines = new Set();

  function advance() {
    if (source[index] === "\n") line += 1;
    index += 1;
  }

  function record(start, startLine) {
    tokens.push(source.slice(start, index));
    codeLines.add(startLine);
  }

  while (index < source.length) {
    const character = source[index];
    const next = source[index + 1];

    if (/\s/.test(character)) {
      advance();
      continue;
    }

    if (character === "/" && next === "/") {
      while (index < source.length && source[index] !== "\n") advance();
      continue;
    }

    if (character === "/" && next === "*") {
      advance();
      advance();
      while (
        index < source.length &&
        !(source[index] === "*" && source[index + 1] === "/")
      ) {
        advance();
      }
      if (index < source.length) {
        advance();
        advance();
      }
      continue;
    }

    const start = index;
    const startLine = line;

    if (character === "'" || character === '"' || character === "`") {
      const quote = character;
      advance();
      while (index < source.length) {
        const current = source[index];
        advance();
        if (current === "\\" && index < source.length) {
          advance();
        } else if (current === quote) {
          break;
        }
      }
      record(start, startLine);
      continue;
    }

    if (/[A-Za-z_]/.test(character)) {
      advance();
      while (index < source.length && /[A-Za-z0-9_]/.test(source[index])) {
        advance();
      }
      record(start, startLine);
      continue;
    }

    if (/[0-9]/.test(character) || (character === "." && /[0-9]/.test(next))) {
      advance();
      while (index < source.length && /[0-9]/.test(source[index])) advance();
      if (source[index] === ".") {
        advance();
        while (index < source.length && /[0-9]/.test(source[index])) advance();
      }
      if (source[index] === "e" || source[index] === "E") {
        advance();
        if (source[index] === "+" || source[index] === "-") advance();
        while (index < source.length && /[0-9]/.test(source[index])) advance();
      }
      if (source[index] === "n") advance();
      record(start, startLine);
      continue;
    }

    const operator = MULTI_CHARACTER_TOKENS.find((candidate) =>
      source.startsWith(candidate, index),
    );
    if (operator !== undefined) {
      for (let offset = 0; offset < operator.length; offset += 1) advance();
      record(start, startLine);
      continue;
    }

    advance();
    record(start, startLine);
  }

  return { tokens, codeLines };
}

function measure(source) {
  const { tokens, codeLines } = lex(source);
  return {
    physicalLines:
      source.length === 0
        ? 0
        : source.split("\n").length - (source.endsWith("\n") ? 1 : 0),
    codeLines: codeLines.size,
    approximateTokens: tokens.length,
    bytes: Buffer.byteLength(source),
  };
}

function addMetrics(left, right) {
  return {
    physicalLines: left.physicalLines + right.physicalLines,
    codeLines: left.codeLines + right.codeLines,
    approximateTokens: left.approximateTokens + right.approximateTokens,
    bytes: left.bytes + right.bytes,
  };
}

function emptyMetrics() {
  return {
    physicalLines: 0,
    codeLines: 0,
    approximateTokens: 0,
    bytes: 0,
  };
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function packetFiles(protocol, armName) {
  const arm = protocol.arms[armName];
  if (arm === undefined) fail(`unknown arm: ${armName}`);
  return [
    ...protocol.commonPacketFiles,
    ...arm.teachingFiles,
    ...protocol.taskFiles,
    ...arm.scaffoldFiles,
    protocol.responseFile,
  ];
}

function packetDigest(protocol, armName) {
  const hash = createHash("sha256");
  for (const name of packetFiles(protocol, armName)) {
    const content = readFileSync(resolveFromRoot(name));
    hash.update(name);
    hash.update("\0");
    hash.update(String(content.byteLength));
    hash.update("\0");
    hash.update(content);
    hash.update("\0");
  }
  return hash.digest("hex");
}

function renderPacket(protocol, armName, runId) {
  const arm = protocol.arms[armName];
  const digest = packetDigest(protocol, armName);
  const required = arm.requiredSubmissionFiles.map((name) => `- ${name}`).join("\n");
  const sections = packetFiles(protocol, armName).map((name) => {
    const content = readFileSync(resolveFromRoot(name), "utf8").trimEnd();
    return `\n\n---\n\n## Packet file: ${name}\n\n${content}`;
  });

  return {
    digest,
    prompt:
      `# Bounded language-acquisition run\n\n` +
      `- Protocol: \`${protocol.id}\`\n` +
      `- Run: \`${runId}\`\n` +
      `- Arm: \`${armName}\` — ${arm.label}\n` +
      `- Phase: paper; no parser or checker result is implied\n` +
      `- External tools: none\n` +
      `- Repair opportunities after first adjudication: ` +
      `${protocol.budgets.repairOpportunities}\n` +
      `- Static packet SHA-256: \`${digest}\`\n\n` +
      `Required files in the first response:\n\n${required}\n\n` +
      `Use only this packet. Do not browse or inspect a repository. Complete ` +
      `all tasks and return the exact submission files. ` +
      `Do not expose chain-of-thought.` +
      sections.join("") +
      "\n",
  };
}

function validateProtocol() {
  const protocol = readProtocol();
  if (protocol.phase !== "paper") fail("protocol phase must be paper");
  if (!protocol.id || typeof protocol.id !== "string") {
    fail("protocol requires a string id");
  }

  const referenced = [
    ...protocol.commonPacketFiles,
    ...protocol.taskFiles,
    protocol.responseFile,
    ...protocol.frozenSources,
    protocol.typescriptBaseline.source,
    protocol.typescriptBaseline.tests,
    protocol.typescriptBaseline.documentation,
    protocol.oracles.comprehension,
    protocol.oracles.semanticRubric,
    protocol.oracles.falseFriends,
  ];
  for (const arm of Object.values(protocol.arms)) {
    referenced.push(...arm.teachingFiles, ...arm.scaffoldFiles);
    unique(arm.requiredSubmissionFiles, "required submission filename");
  }
  unique(referenced, "referenced protocol path");
  for (const name of referenced) assertRegularFile(resolveFromRoot(name));

  const comprehension = readJson(
    resolveFromRoot(protocol.oracles.comprehension),
  );
  const rubric = readJson(resolveFromRoot(protocol.oracles.semanticRubric));
  const falseFriends = readJson(
    resolveFromRoot(protocol.oracles.falseFriends),
  );

  if (comprehension.items?.length !== 10) {
    fail("comprehension oracle must contain exactly 10 items");
  }
  if (rubric.validity?.length !== 4) {
    fail("semantic rubric must contain exactly 4 validity items");
  }
  if (rubric.semantic?.length !== 33) {
    fail("semantic rubric must contain exactly 33 semantic items");
  }
  unique(comprehension.items.map((item) => item.id), "comprehension id");
  unique(rubric.validity.map((item) => item.id), "validity id");
  unique(rubric.semantic.map((item) => item.id), "semantic id");

  const report = {
    protocol: protocol.id,
    phase: protocol.phase,
    arms: {},
    frozenSources: {},
    typescriptBaseline: {},
  };

  for (const [armName, arm] of Object.entries(protocol.arms)) {
    const probes = falseFriends.arms?.[armName];
    if (!Array.isArray(probes) || probes.length !== 10) {
      fail(`${armName}: false-friend oracle must contain exactly 10 items`);
    }
    unique(probes.map((probe) => probe.id), `${armName} false-friend id`);

    if (protocol.leakage.scanCommonAndTeachingFiles) {
      const scanFiles = [
        ...protocol.commonPacketFiles,
        ...arm.teachingFiles,
      ];
      for (const name of scanFiles) {
        const lower = readFileSync(resolveFromRoot(name), "utf8").toLowerCase();
        for (const term of protocol.leakage.bannedCaseInsensitiveTerms) {
          if (lower.includes(term.toLowerCase())) {
            fail(`${name}: teaching-packet leakage term "${term}"`);
          }
        }
      }
    }

    const rendered = renderPacket(protocol, armName, "CHECK-RUN");
    const metrics = measure(rendered.prompt);
    if (
      metrics.approximateTokens >
      protocol.budgets.maxApproximateInputTokens
    ) {
      fail(
        `${armName}: packet has ${metrics.approximateTokens} approximate ` +
          `tokens; budget is ${protocol.budgets.maxApproximateInputTokens}`,
      );
    }
    report.arms[armName] = {
      label: arm.label,
      packetSha256: rendered.digest,
      packetMetrics: metrics,
      requiredSubmissionFiles: arm.requiredSubmissionFiles,
    };
  }

  for (const name of protocol.frozenSources) {
    const content = readFileSync(resolveFromRoot(name));
    report.frozenSources[name] = sha256(content);
  }
  for (const [kind, name] of Object.entries(protocol.typescriptBaseline)) {
    report.typescriptBaseline[kind] = {
      path: name,
      sha256: sha256(readFileSync(resolveFromRoot(name))),
    };
  }

  return { protocol, report, rubric, comprehension, falseFriends };
}

function parseOptions(arguments_) {
  const positionals = [];
  const options = new Map();
  for (let index = 0; index < arguments_.length; index += 1) {
    const value = arguments_[index];
    if (!value.startsWith("--")) {
      positionals.push(value);
      continue;
    }
    const name = value.slice(2);
    const next = arguments_[index + 1];
    if (next === undefined || next.startsWith("--")) {
      options.set(name, true);
    } else {
      options.set(name, next);
      index += 1;
    }
  }
  return { positionals, options };
}

function requireOption(options, name) {
  const value = options.get(name);
  if (typeof value !== "string" || value.length === 0) {
    fail(`missing --${name}`);
  }
  return value;
}

function writeOrPrint(value, out) {
  if (typeof out === "string") {
    writeFileSync(resolve(out), value);
  } else {
    process.stdout.write(value);
  }
}

function validateAnswerJson(path, expectedIds, kind) {
  const value = readJson(path);
  if (kind === "comprehension") {
    if (value === null || Array.isArray(value) || typeof value !== "object") {
      fail(`${path}: expected one object`);
    }
    const keys = Object.keys(value);
    unique(keys, `${path} key`);
    for (const id of expectedIds) {
      if (!(id in value) || typeof value[id] !== "string") {
        fail(`${path}: expected short string answer for ${id}`);
      }
    }
    if (keys.some((key) => !expectedIds.includes(key))) {
      fail(`${path}: unexpected comprehension key`);
    }
    return;
  }

  if (!Array.isArray(value) || value.length !== expectedIds.length) {
    fail(`${path}: expected ${expectedIds.length} false-friend answers`);
  }
  const ids = value.map((item) => item?.id);
  unique(ids, `${path} false-friend id`);
  for (const id of expectedIds) {
    const item = value.find((candidate) => candidate?.id === id);
    if (
      item === undefined ||
      typeof item.problem !== "string" ||
      typeof item.replacement !== "string"
    ) {
      fail(`${path}: incomplete false-friend answer ${id}`);
    }
  }
}

function measurePhase(directory, requiredFiles, sourceExtension) {
  let metrics = emptyMetrics();
  let allMetrics = emptyMetrics();
  let todoMarkers = 0;
  const files = {};

  for (const name of requiredFiles) {
    const path = join(directory, name);
    assertRegularFile(path);
    const source = readFileSync(path, "utf8");
    const measured = measure(source);
    files[name] = measured;
    allMetrics = addMetrics(allMetrics, measured);
    if (extname(name) === sourceExtension) {
      metrics = addMetrics(metrics, measured);
      todoMarkers += source.split("TRIAL-TODO").length - 1;
    }
  }
  return { source: metrics, all: allMetrics, files, todoMarkers };
}

function validateSubmission(submissionPath, checked = validateProtocol()) {
  const { protocol, comprehension, falseFriends } = checked;
  const directory = resolve(submissionPath);
  assertDirectory(directory);
  const metaPath = join(directory, protocol.submission.metadataFile);
  assertRegularFile(metaPath);
  const meta = readJson(metaPath);

  for (const property of [
    "protocol",
    "run",
    "arm",
    "phase",
    "model",
    "packet_sha256",
  ]) {
    if (typeof meta[property] !== "string" || meta[property].length === 0) {
      fail(`${metaPath}: missing string property ${property}`);
    }
  }
  if (meta.protocol !== protocol.id) fail(`${metaPath}: protocol mismatch`);
  if (meta.phase !== protocol.phase) fail(`${metaPath}: phase mismatch`);
  const arm = protocol.arms[meta.arm];
  if (arm === undefined) fail(`${metaPath}: unknown arm ${meta.arm}`);
  const expectedDigest = packetDigest(protocol, meta.arm);
  if (meta.packet_sha256 !== expectedDigest) {
    fail(`${metaPath}: packet_sha256 does not match the current packet`);
  }

  const comprehensionIds = comprehension.items.map((item) => item.id);
  const falseFriendIds = falseFriends.arms[meta.arm].map((item) => item.id);
  const firstDirectory = join(directory, protocol.submission.firstDirectory);
  assertDirectory(firstDirectory);
  const first = measurePhase(
    firstDirectory,
    arm.requiredSubmissionFiles,
    arm.sourceExtension,
  );
  validateAnswerJson(
    join(firstDirectory, "00-comprehension.json"),
    comprehensionIds,
    "comprehension",
  );
  validateAnswerJson(
    join(firstDirectory, "04-false-friends.json"),
    falseFriendIds,
    "falseFriends",
  );
  if (
    first.all.approximateTokens >
    protocol.budgets.maxApproximateFirstOutputTokens
  ) {
    fail(
      `first response exceeds output budget: ` +
        `${first.all.approximateTokens} approximate tokens`,
    );
  }

  const repairDirectory = join(directory, protocol.submission.repairDirectory);
  let repair = null;
  if (existsSync(repairDirectory)) {
    assertDirectory(repairDirectory);
    const diagnosticsPath = join(
      directory,
      protocol.submission.diagnosticsFile,
    );
    assertRegularFile(diagnosticsPath);
    const diagnostics = readJson(diagnosticsPath);
    if (!Array.isArray(diagnostics)) {
      fail(`${diagnosticsPath}: expected a diagnostics array`);
    }
    repair = measurePhase(
      repairDirectory,
      arm.requiredSubmissionFiles,
      arm.sourceExtension,
    );
    validateAnswerJson(
      join(repairDirectory, "00-comprehension.json"),
      comprehensionIds,
      "comprehension",
    );
    validateAnswerJson(
      join(repairDirectory, "04-false-friends.json"),
      falseFriendIds,
      "falseFriends",
    );
    if (
      repair.all.approximateTokens >
      protocol.budgets.maxApproximateRepairOutputTokens
    ) {
      fail(
        `repair response exceeds output budget: ` +
          `${repair.all.approximateTokens} approximate tokens`,
      );
    }
  }

  return {
    directory,
    meta,
    arm,
    expectedDigest,
    first,
    repair,
  };
}

function exactBooleanMap(value, ids, label) {
  if (value === null || Array.isArray(value) || typeof value !== "object") {
    fail(`${label}: expected an object`);
  }
  const keys = Object.keys(value);
  unique(keys, `${label} key`);
  for (const id of ids) {
    if (typeof value[id] !== "boolean") {
      fail(`${label}: expected Boolean ${id}`);
    }
  }
  if (keys.some((key) => !ids.includes(key))) {
    fail(`${label}: unexpected key`);
  }
  return value;
}

function exactFalseFriendMap(value, ids, label) {
  if (value === null || Array.isArray(value) || typeof value !== "object") {
    fail(`${label}: expected an object`);
  }
  const keys = Object.keys(value);
  for (const id of ids) {
    const item = value[id];
    if (
      item === null ||
      typeof item !== "object" ||
      typeof item.recognized !== "boolean" ||
      typeof item.repaired !== "boolean"
    ) {
      fail(`${label}: expected recognized/repaired Booleans for ${id}`);
    }
  }
  if (keys.some((key) => !ids.includes(key))) {
    fail(`${label}: unexpected key`);
  }
  return value;
}

function countTrue(map) {
  return Object.values(map).filter(Boolean).length;
}

function scoreStage(stage, validityIds, semanticIds, falseFriendIds, label) {
  if (stage === null || Array.isArray(stage) || typeof stage !== "object") {
    fail(`${label}: expected an object`);
  }
  const validity = exactBooleanMap(
    stage.validity,
    validityIds,
    `${label}.validity`,
  );
  const semantic = exactBooleanMap(
    stage.semantic,
    semanticIds,
    `${label}.semantic`,
  );
  const falseFriends = exactFalseFriendMap(
    stage.falseFriends,
    falseFriendIds,
    `${label}.falseFriends`,
  );
  if (typeof stage.authority !== "boolean") {
    fail(`${label}.authority: expected Boolean`);
  }
  const recognized = Object.values(falseFriends).filter(
    (item) => item.recognized,
  ).length;
  const repaired = Object.values(falseFriends).filter(
    (item) => item.repaired,
  ).length;
  return {
    validity: countTrue(validity),
    validityMaximum: validityIds.length,
    semantic: countTrue(semantic),
    semanticMaximum: semanticIds.length,
    negativeTransferRecognized: recognized,
    negativeTransferRecognizedMaximum: falseFriendIds.length,
    negativeTransferRepaired: repaired,
    negativeTransferRepairedMaximum: falseFriendIds.length,
    authority: stage.authority,
    eligible:
      countTrue(validity) === validityIds.length &&
      countTrue(semantic) === semanticIds.length &&
      stage.authority,
  };
}

function stringArray(value, label) {
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    fail(`${label}: expected an array of strings`);
  }
  unique(value, `${label} item`);
  return value;
}

function scoreSubmission(submissionPath, adjudicationPath) {
  const checked = validateProtocol();
  const submission = validateSubmission(submissionPath, checked);
  const { protocol, rubric, comprehension, falseFriends } = checked;
  const adjudication = readJson(resolve(adjudicationPath));

  if (
    adjudication.protocol !== protocol.id ||
    adjudication.run !== submission.meta.run ||
    adjudication.arm !== submission.meta.arm
  ) {
    fail(`${adjudicationPath}: protocol, run, or arm mismatch`);
  }

  const comprehensionIds = comprehension.items.map((item) => item.id);
  const validityIds = rubric.validity.map((item) => item.id);
  const semanticIds = rubric.semantic.map((item) => item.id);
  const falseFriendIds = falseFriends.arms[submission.meta.arm].map(
    (item) => item.id,
  );
  const comprehensionMap = exactBooleanMap(
    adjudication.comprehension,
    comprehensionIds,
    "comprehension",
  );
  const first = scoreStage(
    adjudication.first,
    validityIds,
    semanticIds,
    falseFriendIds,
    "first",
  );

  let repair = null;
  if (submission.repair !== null) {
    repair = scoreStage(
      adjudication.repair,
      validityIds,
      semanticIds,
      falseFriendIds,
      "repair",
    );
    const initial = stringArray(
      adjudication.repair.initialDefects,
      "repair.initialDefects",
    );
    const resolvedDefects = stringArray(
      adjudication.repair.resolved,
      "repair.resolved",
    );
    const remaining = stringArray(
      adjudication.repair.remaining,
      "repair.remaining",
    );
    const introduced = stringArray(
      adjudication.repair.introduced,
      "repair.introduced",
    );
    const partition = [...resolvedDefects, ...remaining];
    unique(partition, "repair defect partition");
    if (
      partition.length !== initial.length ||
      partition.some((id) => !initial.includes(id))
    ) {
      fail("repair.resolved and repair.remaining must partition initialDefects");
    }
    repair.defects = {
      initial: initial.length,
      resolved: resolvedDefects.length,
      remaining: remaining.length,
      introduced: introduced.length,
    };
    repair.eligible =
      repair.eligible && remaining.length === 0 && introduced.length === 0;
  } else if (adjudication.repair !== undefined && adjudication.repair !== null) {
    fail("adjudication contains repair scoring but submission has no repair");
  }

  const changedFiles = [];
  if (submission.repair !== null) {
    for (const name of submission.arm.requiredSubmissionFiles) {
      const before = readFileSync(
        join(
          submission.directory,
          protocol.submission.firstDirectory,
          name,
        ),
      );
      const after = readFileSync(
        join(
          submission.directory,
          protocol.submission.repairDirectory,
          name,
        ),
      );
      if (!before.equals(after)) changedFiles.push(name);
    }
  }

  const semanticByTask = {};
  for (const task of ["L0", "L1", "L2", "A0"]) {
    const ids = rubric.semantic
      .filter((item) => item.task === task)
      .map((item) => item.id);
    semanticByTask[task] = {
      first: ids.filter((id) => adjudication.first.semantic[id]).length,
      repair:
        repair === null
          ? null
          : ids.filter((id) => adjudication.repair.semantic[id]).length,
      maximum: ids.length,
    };
  }

  return {
    protocol: protocol.id,
    phase: protocol.phase,
    run: submission.meta.run,
    arm: submission.meta.arm,
    model: submission.meta.model,
    packetSha256: submission.expectedDigest,
    comprehension: {
      score: countTrue(comprehensionMap),
      maximum: comprehensionIds.length,
    },
    first,
    repair,
    semanticByTask,
    burden: {
      first: submission.first.source,
      repair: submission.repair?.source ?? null,
      changedFiles,
      changedFileCount: changedFiles.length,
      firstTodoMarkers: submission.first.todoMarkers,
      repairTodoMarkers: submission.repair?.todoMarkers ?? null,
    },
  };
}

function walkScoreFiles(directory, output = []) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      walkScoreFiles(path, output);
    } else if (
      entry.isFile() &&
      (entry.name === "score.json" || entry.name.endsWith(".score.json"))
    ) {
      output.push(path);
    }
  }
  return output;
}

function median(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
}

function wilson(successes, total) {
  if (total === 0) return null;
  const z = 1.959963984540054;
  const p = successes / total;
  const denominator = 1 + (z * z) / total;
  const center = (p + (z * z) / (2 * total)) / denominator;
  const margin =
    (z *
      Math.sqrt(
        (p * (1 - p)) / total + (z * z) / (4 * total * total),
      )) /
    denominator;
  return {
    rate: p,
    lower95: Math.max(0, center - margin),
    upper95: Math.min(1, center + margin),
  };
}

function summarizeResults(directory) {
  const root = resolve(directory);
  assertDirectory(root);
  const files = walkScoreFiles(root);
  if (files.length === 0) fail(`${root}: no score.json files found`);
  const scores = files.map((path) => readJson(path));
  const protocolIds = [...new Set(scores.map((score) => score.protocol))];
  if (protocolIds.length !== 1) {
    fail("cannot summarize results from different protocol revisions");
  }

  const arms = {};
  for (const armName of [...new Set(scores.map((score) => score.arm))].sort()) {
    const entries = scores.filter((score) => score.arm === armName);
    const repairEntries = entries.filter((score) => score.repair !== null);
    const repairEligible = repairEntries.filter(
      (score) => score.repair.eligible,
    ).length;
    arms[armName] = {
      runs: entries.length,
      models: [...new Set(entries.map((score) => score.model))].sort(),
      medianComprehension: median(
        entries.map((score) => score.comprehension.score),
      ),
      medianFirstValidity: median(
        entries.map((score) => score.first.validity),
      ),
      medianFirstSemantic: median(
        entries.map((score) => score.first.semantic),
      ),
      medianFirstNegativeTransferRepaired: median(
        entries.map((score) => score.first.negativeTransferRepaired),
      ),
      firstEligibility: wilson(
        entries.filter((score) => score.first.eligible).length,
        entries.length,
      ),
      repairEligibility: wilson(repairEligible, repairEntries.length),
      medianRepairDefectsRemaining: median(
        repairEntries.map((score) => score.repair.defects.remaining),
      ),
      medianRepairDefectsIntroduced: median(
        repairEntries.map((score) => score.repair.defects.introduced),
      ),
      medianFirstSourceTokens: median(
        entries.map((score) => score.burden.first.approximateTokens),
      ),
      medianChangedFiles: median(
        repairEntries.map((score) => score.burden.changedFileCount),
      ),
    };
  }

  return {
    protocol: protocolIds[0],
    scoreFiles: files.map((path) => relative(root, path)).sort(),
    arms,
  };
}

function printUsage() {
  console.error(
    [
      "usage:",
      "  node run.mjs check [--out REPORT.json]",
      "  node run.mjs prepare --arm rust|typescript --run ID [--out PROMPT.md]",
      "  node run.mjs validate SUBMISSION_DIR [--out REPORT.json]",
      "  node run.mjs score SUBMISSION_DIR ADJUDICATION.json [--out SCORE.json]",
      "  node run.mjs summarize RESULTS_DIR [--out SUMMARY.json]",
    ].join("\n"),
  );
}

function jsonOutput(value, out) {
  writeOrPrint(`${JSON.stringify(value, null, 2)}\n`, out);
}

function main() {
  const command = process.argv[2];
  const { positionals, options } = parseOptions(process.argv.slice(3));
  const out = options.get("out");

  switch (command) {
    case "check": {
      if (positionals.length !== 0) fail("check accepts no positional arguments");
      jsonOutput(validateProtocol().report, out);
      return;
    }

    case "prepare": {
      if (positionals.length !== 0) {
        fail("prepare accepts only named arguments");
      }
      const arm = requireOption(options, "arm");
      const run = requireOption(options, "run");
      if (!/^[A-Za-z0-9._-]+$/.test(run)) {
        fail("--run must contain only letters, digits, dot, underscore, or dash");
      }
      const { protocol } = validateProtocol();
      const rendered = renderPacket(protocol, arm, run);
      writeOrPrint(rendered.prompt, out);
      return;
    }

    case "validate": {
      if (positionals.length !== 1) {
        fail("validate requires SUBMISSION_DIR");
      }
      const submission = validateSubmission(positionals[0]);
      jsonOutput(
        {
          protocol: submission.meta.protocol,
          run: submission.meta.run,
          arm: submission.meta.arm,
          packetSha256: submission.expectedDigest,
          first: submission.first,
          repair: submission.repair,
        },
        out,
      );
      return;
    }

    case "score": {
      if (positionals.length !== 2) {
        fail("score requires SUBMISSION_DIR and ADJUDICATION.json");
      }
      jsonOutput(scoreSubmission(positionals[0], positionals[1]), out);
      return;
    }

    case "summarize": {
      if (positionals.length !== 1) {
        fail("summarize requires RESULTS_DIR");
      }
      jsonOutput(summarizeResults(positionals[0]), out);
      return;
    }

    default:
      printUsage();
      process.exitCode = 2;
  }
}

try {
  main();
} catch (error) {
  console.error(`acquisition: ${error.message}`);
  process.exitCode = 1;
}
