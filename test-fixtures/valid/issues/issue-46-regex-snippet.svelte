<script lang="ts">
  // Issue #46: Snippet with @const and regex
  let y_label_full = $state("Energy (eV)");
  let x_positions: Record<string, [number, number]> = $state({ segment_1: [0, 1] });

  function pretty_sym_point(label: string): string {
    return label.toUpperCase();
  }
</script>

<div class="parent">
  {#snippet tooltip({ x, y_formatted }: { x: number; y_formatted: string })}
    {@const [, y_label, y_unit] = y_label_full.match(/^(.+?)\s*\(([^)]+)\)$/) ??
      [, y_label_full, ""]}
    {@const segment = Object.entries(x_positions ?? {}).find(([, [start, end]]) =>
      x >= start && x <= end
    )}
    {@const path = segment?.[0].split("_").map((lbl) =>
      lbl !== "null" ? pretty_sym_point(lbl) : ""
    ).filter(Boolean).join(" > ") || null}
    <span>{y_label}: {y_formatted} {y_unit}</span>
    {#if path}<span>Path: {path}</span>{/if}
  {/snippet}

  {@render tooltip({ x: 0.5, y_formatted: "1.23" })}
</div>
