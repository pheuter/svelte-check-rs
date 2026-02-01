import Component from './SnapshotExport.svelte'
import { mount } from 'svelte'

// Test: Accessing exported `snapshot` from a mounted component instance
// This should NOT produce an error, but currently does:
// "Property 'snapshot' does not exist on type '{}'"

const component = mount(Component, {
  target: document.body,
  props: {},
})

// These lines should work - the component exports `snapshot`
const captured = component.snapshot.capture()
component.snapshot.restore({ count: 5 })
