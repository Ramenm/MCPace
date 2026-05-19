export function compareScoreKey(left, right) {
  return right.score - left.score || String(left.key).localeCompare(String(right.key));
}

export function insertTopK(items, item, limit, compare = compareScoreKey) {
  if (!Number.isSafeInteger(limit) || limit <= 0) {
    return false;
  }

  if (items.length < limit) {
    items.push(item);
    items.sort(compare);
    return true;
  }

  const worst = items[items.length - 1];
  if (compare(item, worst) >= 0) {
    return false;
  }

  items[items.length - 1] = item;
  items.sort(compare);
  return true;
}
