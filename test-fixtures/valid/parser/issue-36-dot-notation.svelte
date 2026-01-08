<script>
  import { T } from '@threlte/core'
</script>

<!-- Issue #36: Dot notation in component names (T.Mesh) -->
<T.Mesh rotation.x={Math.PI / 2}>
  <T.BoxGeometry />
</T.Mesh>

<!-- Additional test cases for dot notation -->
<T.Group>
  <T.DirectionalLight position.y={10} />
</T.Group>
