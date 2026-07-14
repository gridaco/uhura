const utf8 = new TextEncoder();

/** Locale-independent ordering matching lexicographic UTF-8 byte order. */
export const compareUtf8 = (left: string, right: string): number => {
  if (left === right) return 0;
  const leftBytes = utf8.encode(left);
  const rightBytes = utf8.encode(right);
  const sharedLength = Math.min(leftBytes.length, rightBytes.length);
  for (let index = 0; index < sharedLength; index += 1) {
    const difference = (leftBytes[index] ?? 0) - (rightBytes[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return leftBytes.length - rightBytes.length;
};
