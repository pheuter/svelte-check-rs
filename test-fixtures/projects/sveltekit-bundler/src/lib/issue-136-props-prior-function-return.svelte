<script lang="ts">
	function isAdmin(role: string): role is 'admin' {
		return role === 'admin';
	}

	let { role = 'user' } = $props();
</script>

<p>{role}</p>
