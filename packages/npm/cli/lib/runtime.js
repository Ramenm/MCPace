export const MINIMUM_NODE_MAJOR = 22;
export const SUPPORTED_NODE_LTS_MAJORS = [22, 24];

export function parseNodeMajor(version = process.versions.node) {
  const match = String(version).trim().match(/^(\d+)/);
  return match ? Number(match[1]) : Number.NaN;
}

export function isSupportedNodeMajor(major) {
  return Number.isInteger(major) && major >= MINIMUM_NODE_MAJOR;
}

export function formatUnsupportedNodeMessage(version = process.versions.node) {
  return (
    `@mcpace/cli requires Node.js ${MINIMUM_NODE_MAJOR} or newer. ` +
    `Detected ${version}. Use Node 22 or Node 24 LTS.`
  );
}

export function assertSupportedNodeVersion(version = process.versions.node) {
  const major = parseNodeMajor(version);
  if (isSupportedNodeMajor(major)) {
    return major;
  }

  const error = new Error(formatUnsupportedNodeMessage(version));
  error.code = 'MCPACE_UNSUPPORTED_NODE';
  throw error;
}
