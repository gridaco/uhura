#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const multiCharacterTokens = [
  "!==",
  "===",
  "...",
  "=>",
  "->",
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

function isIdentifierStart(character) {
  return /[A-Za-z_]/.test(character);
}

function isIdentifierContinue(character) {
  return /[A-Za-z0-9_]/.test(character);
}

function isDecimalDigit(character) {
  return /[0-9]/.test(character);
}

function measure(source) {
  let index = 0;
  let line = 1;
  let tokens = 0;
  const codeLines = new Set();

  function advance() {
    if (source[index] === "\n") line += 1;
    index += 1;
  }

  function recordToken(startLine) {
    tokens += 1;
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

    const startLine = line;

    if (character === "'" || character === '"' || character === "`") {
      const quote = character;
      advance();
      while (index < source.length) {
        const stringCharacter = source[index];
        advance();
        if (stringCharacter === "\\") {
          if (index < source.length) advance();
        } else if (stringCharacter === quote) {
          break;
        }
      }
      recordToken(startLine);
      continue;
    }

    if (isIdentifierStart(character)) {
      advance();
      while (
        index < source.length &&
        isIdentifierContinue(source[index])
      ) {
        advance();
      }
      recordToken(startLine);
      continue;
    }

    if (
      isDecimalDigit(character) ||
      (character === "." && isDecimalDigit(next))
    ) {
      advance();
      while (index < source.length && isDecimalDigit(source[index])) {
        advance();
      }
      if (source[index] === ".") {
        advance();
        while (index < source.length && isDecimalDigit(source[index])) {
          advance();
        }
      }
      if (source[index] === "e" || source[index] === "E") {
        advance();
        if (source[index] === "+" || source[index] === "-") advance();
        while (index < source.length && isDecimalDigit(source[index])) {
          advance();
        }
      }
      if (source[index] === "n") advance();
      recordToken(startLine);
      continue;
    }

    const operator = multiCharacterTokens.find((candidate) =>
      source.startsWith(candidate, index)
    );
    if (operator !== undefined) {
      for (let offset = 0; offset < operator.length; offset += 1) advance();
      recordToken(startLine);
      continue;
    }

    advance();
    recordToken(startLine);
  }

  return {
    physicalLines:
      source.length === 0
        ? 0
        : source.split("\n").length - (source.endsWith("\n") ? 1 : 0),
    codeLines: codeLines.size,
    approximateTokens: tokens,
    bytes: Buffer.byteLength(source),
  };
}

function measurePath(path) {
  const source = readFileSync(path, "utf8");
  return { path, ...measure(source) };
}

function printUsage() {
  console.error(
    "usage: bun run measure-source.mjs SOURCE...\n" +
      "       bun run measure-source.mjs --check METRICS.json",
  );
}

function checkMetrics(metricsPath) {
  const document = JSON.parse(readFileSync(metricsPath, "utf8"));
  if (!Array.isArray(document.sources)) {
    throw new TypeError("metrics document requires a sources array");
  }

  let failed = false;
  for (const expected of document.sources) {
    const path = resolve(dirname(metricsPath), expected.path);
    const actual = measurePath(path);
    console.log(JSON.stringify(actual));

    for (const property of [
      "physicalLines",
      "codeLines",
      "approximateTokens",
      "bytes",
    ]) {
      if (actual[property] !== expected[property]) {
        console.error(
          `${expected.path}: expected ${property}=${expected[property]}, ` +
            `measured ${actual[property]}`,
        );
        failed = true;
      }
    }
  }

  if (failed) process.exitCode = 1;
}

const arguments_ = process.argv.slice(2);
if (arguments_[0] === "--check") {
  if (arguments_.length !== 2 || arguments_[1] === undefined) {
    printUsage();
    process.exitCode = 2;
  } else {
    checkMetrics(arguments_[1]);
  }
} else if (arguments_.length === 0) {
  printUsage();
  process.exitCode = 2;
} else {
  for (const path of arguments_) {
    console.log(JSON.stringify(measurePath(path)));
  }
}
