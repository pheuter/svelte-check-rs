<script lang="ts">
  // Issue #93: Type narrowing after throw should be recognized in template
  // https://github.com/pheuter/svelte-check-rs/issues/93

  interface DataFile {
    figshare: string;
    name: string;
  }

  const data_files: Record<string, DataFile | undefined> = {
    mp_trj_json_gz: { figshare: "https://example.com/data", name: "trajectory" }
  };

  const mp_trj_data = data_files["mp_trj_json_gz"];

  // Type narrowing via throw - after this, mp_trj_data should be DataFile
  if (!mp_trj_data) {
    throw new Error("mp_trj_json_gz not found");
  }

  // At this point, mp_trj_data is narrowed to DataFile (not undefined)
</script>

<!-- Template should see mp_trj_data as DataFile, not DataFile | undefined -->
<a href={mp_trj_data.figshare}>{mp_trj_data.name}</a>
