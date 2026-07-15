export function sameStringRecord(actual, expected) {
	if (
		!actual ||
		!expected ||
		typeof actual !== "object" ||
		typeof expected !== "object" ||
		Array.isArray(actual) ||
		Array.isArray(expected)
	) {
		return false;
	}
	const actualKeys = Object.keys(actual).sort();
	const expectedKeys = Object.keys(expected).sort();
	return (
		actualKeys.length === expectedKeys.length &&
		actualKeys.every((key, index) => key === expectedKeys[index]) &&
		expectedKeys.every(
			(key) => typeof actual[key] === "string" && actual[key] === expected[key],
		)
	);
}
