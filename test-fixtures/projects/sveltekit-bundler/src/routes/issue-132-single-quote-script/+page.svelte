<script lang='ts'>
  type ZodInfer<T> = T extends { _output: infer O } ? O : never;

  function object<T>(_shape: T): { _output: T } {
    return null as any;
  }
  function string(): { _output: string } {
    return null as any;
  }

  const z = { object, string };

  const schema = z.object({
    username: z.string(),
    password: z.string()
  });

  type Form = ZodInfer<typeof schema>;

  let form = $state<Form | null>(null);
</script>

<pre>{JSON.stringify(form)}</pre>
