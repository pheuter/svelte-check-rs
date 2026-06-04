<script lang="ts">
  // A renamed bindable prop whose exported name (`class`) is a reserved word.
  // The local binding `className` is never read in the template, so under
  // `noUnusedLocals: true` it would be flagged TS6133 unless the transformer
  // marks it used via its LOCAL name (upstream #3017). Using the exported
  // reserved word `class` in the marker (`;class;`) would itself be a TS
  // syntax error, which is the whole point of #3017.
  // Regression for the bindable mark-used mechanism.
  //
  // `unusedProp` is a NON-bindable companion in the SAME destructuring. It is
  // also never read, but only $bindable() props receive the mark-used
  // reference, so `unusedProp` MUST still surface TS6133 — proving the
  // mark-used is bindable-targeted and not a blanket suppression of every
  // destructured prop.
  let {
    class: className = $bindable(),
    unusedProp
  }: { class?: string; unusedProp?: string } = $props();
</script>
