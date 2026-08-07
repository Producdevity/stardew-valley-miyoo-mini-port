export function steamQrMatrix(lines: string[]): boolean[][] {
  return lines.map((line) =>
    Array.from({ length: Math.ceil(line.length / 2) }, (_, column) =>
      line.slice(column * 2, column * 2 + 2).includes("█"),
    ),
  );
}
