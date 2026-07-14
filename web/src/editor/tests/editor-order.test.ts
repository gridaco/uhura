import assert from "node:assert/strict";
import { test } from "vitest";

import { compareUtf8 } from "../editor-order.js";

test("orders identifiers by UTF-8 bytes without locale collation", () => {
  const values = ["é", "a_", "a-", "a", "Z"];
  assert.deepEqual(values.toSorted(compareUtf8), ["Z", "a", "a-", "a_", "é"]);
  assert.equal(compareUtf8("same", "same"), 0);
});
