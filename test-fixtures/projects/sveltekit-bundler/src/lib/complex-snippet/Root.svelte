<script lang="ts">
  import type { Snippet } from 'svelte'

  export type Month = {
    value: string
    weeks: string[][]
  }

  type Single = {
    type: 'single'
    value?: string
    onValueChange?: (value: string | undefined) => void
  }

  type Multiple = {
    type: 'multiple'
    value?: string[]
    onValueChange?: (value: string[]) => void
  }

  export type RootProps = (Single | Multiple) & {
    months?: Month[]
    weekdays?: string[]
    children?: Snippet<[{ months: Month[]; weekdays: string[] }]>
  }

  let {
    children,
    months = [{ value: '2026-01', weeks: [['2026-01-01']] }],
    weekdays = ['Mon', 'Tue'],
  }: RootProps = $props()
</script>

{#if children}
  {@render children({ months, weekdays })}
{/if}
