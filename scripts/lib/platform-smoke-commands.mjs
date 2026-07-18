export const PLATFORM_SMOKE_COMMANDS = Object.freeze([
	{
		args: ["help"],
		expects: "text",
		reason: "small public command router and help",
	},
	{ args: ["version"], expects: "text", reason: "version metadata path" },
	{
		args: ["up", "--help"],
		expects: "text",
		reason: "convergent onboarding help",
	},
	{
		args: ["start", "--help"],
		expects: "text",
		reason: "start lifecycle help",
	},
	{
		args: ["stop", "--help"],
		expects: "text",
		reason: "stop lifecycle help",
	},
	{
		args: ["restart", "--help"],
		expects: "text",
		reason: "restart lifecycle help",
	},
	{
		args: ["install", "--help"],
		expects: "text",
		reason: "server install help",
	},
	{
		args: ["status", "--json"],
		expects: "jsonOrStatusOne",
		reason: "read-only aggregate runtime/startup status",
	},
	{
		args: ["advanced", "doctor", "--json"],
		expects: "json",
		reason: "host and source readiness without runtime start",
	},
	{
		args: ["advanced", "server", "list", "--json"],
		expects: "json",
		reason: "server inventory contract",
	},
	{
		args: ["advanced", "server", "capabilities", "--json"],
		expects: "json",
		reason: "launch/capability metadata contract",
	},
	{
		args: ["advanced", "server", "sources", "--json"],
		expects: "json",
		reason: "MCP settings source discovery contract",
	},
	{
		args: ["advanced", "client", "list", "--json"],
		expects: "json",
		reason: "client catalog visibility contract",
	},
	{
		args: ["advanced", "client", "plan", "--json"],
		expects: "json",
		reason: "client routing plan contract",
	},
	{
		args: ["advanced", "dev", "profile", "--json"],
		expects: "json",
		reason: "maintainer runtime profile read contract",
	},
	{
		args: ["advanced", "dev", "projects", "--json"],
		expects: "json",
		reason: "maintainer project registry read contract",
	},
	{
		args: ["advanced", "dev", "lab", "report", "--json"],
		expects: "json",
		reason: "maintainer evidence corpus contract",
	},
	{
		args: ["advanced", "runtime", "cleanup", "status", "--json"],
		expects: "json",
		reason: "non-destructive cleanup plan",
	},
	{
		args: ["advanced", "dev", "release", "--help"],
		expects: "text",
		reason: "maintainer release help path",
	},
	{
		args: ["advanced", "autostart", "--help"],
		expects: "text",
		reason: "platform startup help path",
	},
	{
		args: ["uninstall", "--help"],
		expects: "text",
		reason: "safe integration removal contract",
	},
]);

export function platformSmokeCommands() {
	return PLATFORM_SMOKE_COMMANDS.map((item) => ({
		...item,
		args: [...item.args],
	}));
}
