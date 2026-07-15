import { describe, expect, it } from "vitest";
import {
  formatCost,
  formatInteger,
  formatPercent,
  formatTokens,
} from "./format";

describe("formatTokens", () => {
  it("uses compact suffixes at each threshold", () => {
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1_234)).toBe("1.234K");
    expect(formatTokens(12_000)).toBe("12K");
    expect(formatTokens(1_250_000)).toBe("1.25M");
    expect(formatTokens(2_000_000_000)).toBe("2B");
  });
});

describe("numeric presentation", () => {
  it("formats integer and percentage values", () => {
    expect(formatInteger(12_345)).toBe("12,345");
    expect(formatPercent(0.954)).toBe("95%");
  });

  it("formats costs and rejects invalid values", () => {
    expect(formatCost("9.7654")).toBe("$9.7654");
    expect(formatCost("12.5")).toBe("$12.50");
    expect(formatCost("12.5", 3)).toBe("$12.500");
    expect(formatCost(null)).toBe("未定价");
    expect(formatCost("not-a-number")).toBe("未定价");
  });
});
