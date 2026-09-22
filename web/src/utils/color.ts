export function colorAlpha(color: string, alpha: number): string {
  if (!color) return color
  return `color-mix(in srgb, ${color} ${Math.round(alpha * 100)}%, transparent)`
}
