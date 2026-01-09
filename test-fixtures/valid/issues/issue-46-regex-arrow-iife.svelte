<script lang="ts">
  // Issue #46: @const with arrow function type annotations and IIFE containing regex
  let color_value: number | null = $state(0.5);
  let point_style = $state({ fill: "blue", stroke: "red" });

  function color_scale_fn(value: number): string {
    return `hsl(${value * 360}, 50%, 50%)`;
  }
</script>

{#if color_value !== null}
  {@const is_transparent_or_none = (color: string | undefined | null): boolean =>
    !color ||
    color === "none" ||
    color === "transparent" ||
    /rgba\([^)]+[,/]\s*0(\.0*)?\s*\)$/.test(color)}
  {@const tooltip_bg_color = (() => {
    const scale_color = color_value != null
      ? color_scale_fn(color_value)
      : undefined;
    if (!is_transparent_or_none(scale_color)) return scale_color;
    const fill_color = point_style?.fill;
    if (!is_transparent_or_none(fill_color)) return fill_color;
    return "rgba(0, 0, 0, 0.7)";
  })()}
  <div style="background: {tooltip_bg_color}">Tooltip</div>
{/if}
