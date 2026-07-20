import type { PreviewIdentity } from "./editor-state.js";

export interface PreviewDisplayLabels {
  readonly subject: string;
  readonly example: string;
  readonly combined: string;
}

type SubjectIdentity = Pick<PreviewIdentity, "kind" | "subject">;

const qualifiedTail = (value: string): string => {
  const separator = value.lastIndexOf("::");
  return separator < 0 ? value : value.slice(separator + 2);
};

/**
 * Converts one authored/public identifier into the Editor's compact label
 * vocabulary. This is presentation only: callers retain the original identity
 * for every semantic join and protocol operation.
 */
export const editorIdentifierLabel = (value: string): string => {
  const tail = qualifiedTail(value)
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2")
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/[^A-Za-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .toLowerCase();
  return tail || value;
};

/** A page's `Page` suffix describes its UI kind, so the Editor need not repeat it. */
export const editorSubjectLabel = (identity: SubjectIdentity): string => {
  const label = editorIdentifierLabel(identity.subject);
  if (identity.kind !== "page") return label;
  const withoutPage = label.replace(/-?page$/, "");
  return withoutPage || label;
};

const fullExampleLabel = (identity: PreviewIdentity): string =>
  editorIdentifierLabel(identity.example);

const shortenedExampleLabel = (identity: PreviewIdentity): string => {
  const full = fullExampleLabel(identity);
  const subject = editorSubjectLabel(identity);
  const prefix = `${subject}-`;
  return subject && full.startsWith(prefix) && full.length > prefix.length
    ? full.slice(prefix.length)
    : full;
};

const sameSubject = (left: PreviewIdentity, right: PreviewIdentity): boolean =>
  left.kind === right.kind && left.subject === right.subject;

/**
 * Produces friendly labels for one preview. A presentation-derived subject
 * prefix is removed from its example only when the resulting label is unique
 * among the subject's peer examples.
 */
export const editorPreviewLabels = (
  identity: PreviewIdentity,
  peers: readonly PreviewIdentity[] = [identity],
): PreviewDisplayLabels => {
  const subject = editorSubjectLabel(identity);
  const fullExample = fullExampleLabel(identity);
  const candidate = shortenedExampleLabel(identity);
  const collisions = peers.filter((peer) =>
    sameSubject(identity, peer) && shortenedExampleLabel(peer) === candidate);
  const example = candidate !== fullExample && collisions.length === 1
    ? candidate
    : fullExample;
  return { subject, example, combined: `${subject} / ${example}` };
};
