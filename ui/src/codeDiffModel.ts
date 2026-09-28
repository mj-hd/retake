export type DiffLine = { text: string; kind: 'same' | 'removed' | 'added' };

interface DiffMessages {
  moreRemoved: (count: number) => string;
  moreAdded: (count: number) => string;
}

export function compareLines(before: string[], after: string[], messages: DiffMessages): DiffLine[] {
  let prefix = 0;
  while (prefix < before.length && prefix < after.length && before[prefix] === after[prefix]) prefix++;
  let suffix = 0;
  while (
    suffix < before.length - prefix &&
    suffix < after.length - prefix &&
    before[before.length - suffix - 1] === after[after.length - suffix - 1]
  ) {
    suffix++;
  }
  const left = before.slice(prefix, before.length - suffix);
  const right = after.slice(prefix, after.length - suffix);
  if (!left.length && !right.length) return [];
  const lines: DiffLine[] = before
    .slice(Math.max(0, prefix - 3), prefix)
    .map((text) => ({ text, kind: 'same' as const }));
  if (left.length * right.length > 1_000_000) {
    lines.push(...left.slice(0, 300).map((text) => ({ text, kind: 'removed' as const })));
    if (left.length > 300) lines.push({ text: messages.moreRemoved(left.length - 300), kind: 'same' });
    lines.push(...right.slice(0, 300).map((text) => ({ text, kind: 'added' as const })));
    if (right.length > 300) lines.push({ text: messages.moreAdded(right.length - 300), kind: 'same' });
  } else {
    const matrix = Array.from({ length: left.length + 1 }, () => new Uint32Array(right.length + 1));
    for (let leftIndex = left.length - 1; leftIndex >= 0; leftIndex--) {
      for (let rightIndex = right.length - 1; rightIndex >= 0; rightIndex--) {
        matrix[leftIndex][rightIndex] =
          left[leftIndex] === right[rightIndex]
            ? matrix[leftIndex + 1][rightIndex + 1] + 1
            : Math.max(matrix[leftIndex + 1][rightIndex], matrix[leftIndex][rightIndex + 1]);
      }
    }
    let leftIndex = 0;
    let rightIndex = 0;
    while (leftIndex < left.length || rightIndex < right.length) {
      if (leftIndex < left.length && rightIndex < right.length && left[leftIndex] === right[rightIndex]) {
        lines.push({ text: left[leftIndex++], kind: 'same' });
        rightIndex++;
      } else if (
        leftIndex < left.length &&
        (rightIndex === right.length || matrix[leftIndex + 1][rightIndex] >= matrix[leftIndex][rightIndex + 1])
      ) {
        lines.push({ text: left[leftIndex++], kind: 'removed' });
      } else {
        lines.push({ text: right[rightIndex++], kind: 'added' });
      }
    }
  }
  lines.push(
    ...after
      .slice(after.length - suffix, Math.min(after.length, after.length - suffix + 3))
      .map((text) => ({ text, kind: 'same' as const })),
  );
  return lines;
}
