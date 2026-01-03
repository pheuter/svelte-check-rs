<!-- Valid a11y: Forms and labels that should NOT trigger any warnings -->
<script>
	let name = $state('');
	let email = $state('');
	let message = $state('');
	let agree = $state(false);
	let country = $state('');
</script>

<!-- Labels with 'for' attribute - should NOT trigger a11y-label-has-associated-control -->
<label for="name">Name</label>
<input id="name" type="text" bind:value={name} />

<label for="email">Email Address</label>
<input id="email" type="email" bind:value={email} />

<label for="message">Message</label>
<textarea id="message" bind:value={message}></textarea>

<label for="country">Country</label>
<select id="country" bind:value={country}>
	<option value="">Select a country</option>
	<option value="us">United States</option>
	<option value="uk">United Kingdom</option>
</select>

<label for="agree">I agree to the terms</label>
<input id="agree" type="checkbox" bind:checked={agree} />

<!-- Labels with nested controls - should NOT trigger -->
<label>
	Username
	<input type="text" />
</label>

<label>
	Password
	<input type="password" />
</label>

<label>
	<input type="checkbox" />
	Remember me
</label>

<label>
	<input type="radio" name="option" value="a" />
	Option A
</label>

<label>
	Comments
	<textarea></textarea>
</label>

<label>
	Category
	<select>
		<option>Option 1</option>
		<option>Option 2</option>
	</select>
</label>

<label>
	<button type="button">Action</button>
	Click to perform action
</label>

<!-- Labels with nested controls and additional content -->
<label>
	<span class="label-text">Full Name</span>
	<span class="required">*</span>
	<input type="text" />
</label>

<label>
	Email
	<span class="hint">(We'll never share your email)</span>
	<input type="email" />
</label>

<!-- Implicit labels via aria-labelledby -->
<span id="phone-label">Phone Number</span>
<input type="tel" aria-labelledby="phone-label" />

<!-- Multiple labels for same control -->
<label for="dob">Date of Birth</label>
<label for="dob">(MM/DD/YYYY)</label>
<input id="dob" type="text" />

<!-- Fieldset and legend (not label, but valid grouping) -->
<fieldset>
	<legend>Contact Information</legend>
	<label>
		Name
		<input type="text" />
	</label>
	<label>
		Email
		<input type="email" />
	</label>
</fieldset>

<!-- Elements WITHOUT autofocus - should NOT trigger a11y-autofocus -->
<input type="text" />
<textarea></textarea>
<select><option>Option</option></select>
<button>Submit</button>

<!-- Elements WITHOUT accesskey - should NOT trigger a11y-accesskey -->
<button>Save</button>
<a href="/home">Home</a>
<input type="text" />
