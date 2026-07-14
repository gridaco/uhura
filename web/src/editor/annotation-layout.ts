import { compareUtf8 } from "./editor-order.js";

export interface AnnotationRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface AnnotationPoint {
  x: number;
  y: number;
}

export interface AnnotationLayoutInput {
  id: string;
  sourceOrder: number;
  anchor: AnnotationRect;
  card: { width: number; height: number };
  /** Marker-only inputs participate in marker stacking without reserving card space. */
  showCard?: boolean;
  /** Dense, persistent callouts can opt into stable viewport gutters. */
  preferGutter?: boolean;
}

export interface AnnotationPlacement {
  id: string;
  candidate: AnnotationCandidate;
  marker: AnnotationPoint;
  card: AnnotationRect;
  leaderFrom: AnnotationPoint;
  leaderTo: AnnotationPoint;
  gutter: boolean;
}

export type AnnotationCandidate =
  | "right"
  | "left"
  | "below"
  | "above"
  | "gutter-left"
  | "gutter-right";

const right = (rect: AnnotationRect): number => rect.left + rect.width;
const bottom = (rect: AnnotationRect): number => rect.top + rect.height;
const area = (rect: AnnotationRect): number =>
  Math.max(0, rect.width) * Math.max(0, rect.height);

export const intersectAnnotationRects = (
  left: AnnotationRect,
  rightRect: AnnotationRect,
): AnnotationRect | null => {
  const x1 = Math.max(left.left, rightRect.left);
  const y1 = Math.max(left.top, rightRect.top);
  const x2 = Math.min(right(left), right(rightRect));
  const y2 = Math.min(bottom(left), bottom(rightRect));
  return x2 > x1 && y2 > y1
    ? { left: x1, top: y1, width: x2 - x1, height: y2 - y1 }
    : null;
};

export const clipAnnotationRect = (
  rect: AnnotationRect,
  clips: readonly AnnotationRect[],
): AnnotationRect | null => {
  let visible: AnnotationRect | null = rect;
  for (const clip of clips) {
    if (!visible) return null;
    visible = intersectAnnotationRects(visible, clip);
  }
  return visible;
};

export const unionAnnotationRects = (
  rects: readonly AnnotationRect[],
): AnnotationRect | null => {
  if (rects.length === 0) return null;
  const first = rects[0];
  if (!first) return null;
  let left = first.left;
  let top = first.top;
  let x2 = right(first);
  let y2 = bottom(first);
  for (const rect of rects.slice(1)) {
    left = Math.min(left, rect.left);
    top = Math.min(top, rect.top);
    x2 = Math.max(x2, right(rect));
    y2 = Math.max(y2, bottom(rect));
  }
  return { left, top, width: x2 - left, height: y2 - top };
};

const clamp = (value: number, minimum: number, maximum: number): number =>
  Math.min(Math.max(value, minimum), Math.max(minimum, maximum));

const overflowArea = (rect: AnnotationRect, viewport: AnnotationRect): number =>
  area(rect) - area(intersectAnnotationRects(rect, viewport) ?? {
    left: 0,
    top: 0,
    width: 0,
    height: 0,
  });

const overlapArea = (rect: AnnotationRect, others: readonly AnnotationRect[]): number =>
  others.reduce((total, other) =>
    total + area(intersectAnnotationRects(rect, other) ?? {
      left: 0,
      top: 0,
      width: 0,
      height: 0,
    }), 0);

interface ScoredCandidate {
  key: AnnotationCandidate;
  candidate: AnnotationRect;
  overflow: number;
  collision: number;
  occlusion: number;
  leaderLength: number;
  ordinal: number;
}

const localCandidates = (
  anchor: AnnotationRect,
  card: { width: number; height: number },
  gap: number,
): Array<{ key: AnnotationCandidate; candidate: AnnotationRect }> => [
  { key: "right", candidate: { left: right(anchor) + gap, top: anchor.top, ...card } },
  { key: "left", candidate: { left: anchor.left - card.width - gap, top: anchor.top, ...card } },
  { key: "below", candidate: { left: anchor.left, top: bottom(anchor) + gap, ...card } },
  { key: "above", candidate: { left: anchor.left, top: anchor.top - card.height - gap, ...card } },
];

const leaderLength = (marker: AnnotationPoint, card: AnnotationRect): number => {
  const dx = Math.max(card.left - marker.x, 0, marker.x - right(card));
  const dy = Math.max(card.top - marker.y, 0, marker.y - bottom(card));
  return Math.hypot(dx, dy);
};

const compareCandidateQuality = (
  left: ScoredCandidate,
  rightCandidate: ScoredCandidate,
): number =>
  left.overflow - rightCandidate.overflow
  || left.collision - rightCandidate.collision
  || left.occlusion - rightCandidate.occlusion
  || left.leaderLength - rightCandidate.leaderLength
  || left.ordinal - rightCandidate.ordinal;

const sameHardConstraints = (
  left: ScoredCandidate,
  rightCandidate: ScoredCandidate,
): boolean =>
  left.overflow === rightCandidate.overflow
  && left.collision === rightCandidate.collision
  && left.occlusion === rightCandidate.occlusion;

/**
 * A small screen-space deadband prevents resize and font reflow noise from
 * making a card flip between otherwise equivalent candidates. Hard geometry
 * constraints always win; hysteresis only applies after those are equal.
 */
const selectCandidate = (
  candidates: readonly ScoredCandidate[],
  previous: AnnotationCandidate | undefined,
  hysteresis = 8,
): ScoredCandidate | undefined => {
  const ordered = candidates.toSorted(compareCandidateQuality);
  const best = ordered[0];
  if (!best || !previous) return best;
  const stable = ordered.find((candidate) => candidate.key === previous);
  return stable
      && sameHardConstraints(stable, best)
      && stable.leaderLength <= best.leaderLength + hysteresis
    ? stable
    : best;
};

/**
 * Deterministic screen-space placement. Local candidates are preferred when
 * they fit without card/target overlap; dense cases fall into stable viewport
 * gutters so the callouts never alter preview layout.
 */
export const placeAnnotations = (
  inputs: readonly AnnotationLayoutInput[],
  viewport: AnnotationRect,
  margin = 12,
  gap = 10,
  obstacles: readonly AnnotationRect[] = [],
  previousPlacements: ReadonlyMap<string, AnnotationPlacement> = new Map(),
): AnnotationPlacement[] => {
  const ordered = inputs.toSorted((left, rightInput) =>
    left.anchor.top - rightInput.anchor.top
    || left.sourceOrder - rightInput.sourceOrder
    || compareUtf8(left.id, rightInput.id)
  );
  const cards: AnnotationRect[] = [...obstacles];
  const gutterBottom = { left: viewport.top + margin, right: viewport.top + margin };
  const sharedAnchors = new Map<string, number>();
  const placements: AnnotationPlacement[] = [];

  for (const input of ordered) {
    const anchorKey = [
      input.anchor.left,
      input.anchor.top,
      input.anchor.width,
      input.anchor.height,
    ].map((value) => Math.round(value * 10) / 10).join(":");
    const sharedIndex = sharedAnchors.get(anchorKey) ?? 0;
    sharedAnchors.set(anchorKey, sharedIndex + 1);
    const marker = {
      x: clamp(
        right(input.anchor) + sharedIndex * 7,
        viewport.left + margin,
        right(viewport) - margin,
      ),
      y: clamp(
        input.anchor.top + sharedIndex * 7,
        viewport.top + margin,
        bottom(viewport) - margin,
      ),
    };
    if (input.showCard === false) {
      placements.push({
        id: input.id,
        candidate: "right",
        marker,
        card: { left: marker.x, top: marker.y, width: 0, height: 0 },
        leaderFrom: marker,
        leaderTo: marker,
        gutter: false,
      });
      continue;
    }
    const scored = localCandidates(input.anchor, input.card, gap).map(({ key, candidate }, index) => {
      const overflow = overflowArea(candidate, viewport);
      const collision = overlapArea(candidate, cards);
      const occlusion = area(intersectAnnotationRects(candidate, input.anchor) ?? {
        left: 0,
        top: 0,
        width: 0,
        height: 0,
      });
      return {
        key,
        candidate,
        overflow,
        collision,
        occlusion,
        leaderLength: leaderLength(marker, candidate),
        ordinal: index,
      };
    });
    const previous = previousPlacements.get(input.id)?.candidate;
    const local = input.preferGutter
      ? undefined
      : selectCandidate(
        scored.filter((candidate) =>
          candidate.overflow === 0
          && candidate.collision === 0
          && candidate.occlusion === 0
        ),
        previous,
      );
    let card: AnnotationRect;
    let candidate: AnnotationCandidate;
    let gutter = false;
    if (local) {
      card = local.candidate;
      candidate = local.key;
    } else {
      gutter = true;
      const gutters = (["left", "right"] as const).map((side) => {
        const left = side === "left"
          ? viewport.left + margin
          : right(viewport) - margin - input.card.width;
        let top = clamp(
          Math.max(input.anchor.top, gutterBottom[side]),
          viewport.top + margin,
          bottom(viewport) - margin - input.card.height,
        );
        // Move below fixed chrome and earlier cards when the gutter has room.
        for (let attempt = 0; attempt <= cards.length; attempt += 1) {
          const candidate = { left, top, ...input.card };
          const collisions = cards.filter((other) =>
            intersectAnnotationRects(candidate, other) !== null
          );
          if (collisions.length === 0) break;
          const nextTop = Math.max(...collisions.map((other) => bottom(other))) + gap;
          const moved = clamp(
            nextTop,
            viewport.top + margin,
            bottom(viewport) - margin - input.card.height,
          );
          if (moved === top) break;
          top = moved;
        }
        const gutterCandidate = { left, top, ...input.card };
        return {
          key: `gutter-${side}` as const,
          candidate: gutterCandidate,
          overflow: overflowArea(gutterCandidate, viewport),
          collision: overlapArea(gutterCandidate, cards),
          occlusion: area(intersectAnnotationRects(gutterCandidate, input.anchor) ?? {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
          }),
          leaderLength: leaderLength(marker, gutterCandidate),
          ordinal: side === "left" ? 0 : 1,
        };
      });
      const selected = selectCandidate(gutters, previous);
      if (!selected) throw new Error("annotation layout needs a viewport gutter");
      const side = selected.key === "gutter-left" ? "left" : "right";
      card = selected.candidate;
      candidate = selected.key;
      gutterBottom[side] = card.top + input.card.height + gap;
    }
    cards.push(card);
    placements.push({
      id: input.id,
      candidate,
      marker,
      card,
      leaderFrom: marker,
      leaderTo: {
        x: clamp(marker.x, card.left, right(card)),
        y: clamp(marker.y, card.top, bottom(card)),
      },
      gutter,
    });
  }
  return placements;
};
