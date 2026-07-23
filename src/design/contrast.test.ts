import { describe, expect, it } from "vitest";

function luminance(hex: string): number {
  const channels = hex
    .replace("#", "")
    .match(/.{2}/g)
    ?.map((channel) => Number.parseInt(channel, 16) / 255);
  if (channels?.length !== 3) throw new Error("色彩格式必須是六位十六進位。");
  const [red, green, blue] = channels.map((channel) =>
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrastRatio(foreground: string, background: string): number {
  const brighter = Math.max(luminance(foreground), luminance(background));
  const darker = Math.min(luminance(foreground), luminance(background));
  return (brighter + 0.05) / (darker + 0.05);
}

describe("Design tokens", () => {
  it.each([
    ["亮色主要文字", "#0f172a", "#f5f7fb"],
    ["亮色次要文字", "#475569", "#f5f7fb"],
    ["暗色主要文字", "#f1f5f9", "#0b1120"],
    ["暗色次要文字", "#b6c2d2", "#0b1120"],
    ["主要按鈕", "#ffffff", "#2563eb"],
  ])("%s 對比至少達 WCAG AA", (_name, foreground, background) => {
    expect(contrastRatio(foreground, background)).toBeGreaterThanOrEqual(4.5);
  });
});
