const path = require("node:path");

const packageEntry = process.argv[2];
const password = process.argv[3];
const rounds = Number(process.argv[4] || "12");

if (!packageEntry) {
  console.error("Missing bcryptjs package entry path.");
  process.exit(1);
}

if (typeof password !== "string") {
  console.error("Missing password argument.");
  process.exit(1);
}

const bcrypt = require(path.resolve(packageEntry));
process.stdout.write(String(bcrypt.hashSync(password, rounds)));
