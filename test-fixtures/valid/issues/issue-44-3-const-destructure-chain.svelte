{#snippet tooltip({ x, y_formatted, label, metadata })}
  {@const segment = Object.entries(positions ?? {}).find(([, [start, end]]) =>
    x >= start && x <= end
  )}
  {@const path = segment?.[0].split(`_`).map((lbl) =>
    lbl !== `null` ? pretty(lbl) : ``
  ).filter(Boolean).join(` → `) || null}
{/snippet}
