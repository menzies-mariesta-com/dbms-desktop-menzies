import { z } from 'zod';

export const engineSchema = z.enum([
	'postgresql',
	'mysql',
	'mariadb',
	'mssql',
	'sqlite',
	'mongodb'
]);
export type Engine = z.infer<typeof engineSchema>;

export const envColorSchema = z.enum(['production', 'staging', 'development', 'neutral']);
export type EnvColor = z.infer<typeof envColorSchema>;

export const sslConfigSchema = z.object({
	mode: z.string(),
	caPath: z.string().nullable().optional(),
	clientCertPath: z.string().nullable().optional(),
	clientKeyPath: z.string().nullable().optional()
});

export const sshTunnelSchema = z.object({
	enabled: z.boolean(),
	host: z.string(),
	port: z.number().int(),
	username: z.string(),
	authMethod: z.string(),
	privateKeyPath: z.string().nullable().optional(),
	passwordSecretRef: z.string().nullable().optional()
});

export const connectionDraftSchema = z.object({
	id: z.string().nullable().optional(),
	name: z.string().min(1),
	engine: engineSchema,
	groupName: z.string(),
	envColor: envColorSchema,
	host: z.string().nullable().optional(),
	port: z.number().int().nullable().optional(),
	database: z.string().nullable().optional(),
	username: z.string().nullable().optional(),
	password: z.string().nullable().optional(),
	filePath: z.string().nullable().optional(),
	connectionUri: z.string().nullable().optional(),
	ssl: sslConfigSchema,
	ssh: sshTunnelSchema,
	extraOptions: z.record(z.string(), z.unknown()).default({})
});
export type ConnectionDraft = z.infer<typeof connectionDraftSchema>;

export const connectionSummarySchema = z.object({
	id: z.string(),
	name: z.string(),
	engine: engineSchema,
	groupName: z.string(),
	envColor: envColorSchema,
	host: z.string().nullable().optional(),
	port: z.number().nullable().optional(),
	database: z.string().nullable().optional(),
	username: z.string().nullable().optional(),
	filePath: z.string().nullable().optional(),
	hasPassword: z.boolean(),
	sslMode: z.string(),
	sshEnabled: z.boolean(),
	createdAt: z.string(),
	updatedAt: z.string()
});
export type ConnectionSummary = z.infer<typeof connectionSummarySchema>;

export type TreeNode = {
	id: string;
	kind: string;
	name: string;
	engine: Engine;
	children: TreeNode[];
	meta: Record<string, unknown>;
};

export type QueryResult = {
	columns: string[];
	rows: unknown[][];
	rowCount: number;
	truncated: boolean;
	durationMs: number;
	messages: string[];
	explainText?: string | null;
};

export type ColumnInfo = {
	name: string;
	dataType: string;
	nullable: boolean;
	primaryKey: boolean;
	defaultValue?: string | null;
};

export type AppSettings = {
	confirmDestructive: boolean;
	defaultMaxRows: number;
	editorWrap: boolean;
	autoCommit: boolean;
	onboardingComplete: boolean;
};

export type WorkspaceTab =
	| {
			id: string;
			kind: 'console';
			title: string;
			sql: string;
			connectionId?: string;
			sessionId?: string;
	  }
	| {
			id: string;
			kind: 'data';
			title: string;
			connectionId: string;
			sessionId: string;
			schema?: string;
			table: string;
	  }
	| { id: string; kind: 'erd'; title: string; sessionId: string; schema?: string }
	| { id: string; kind: 'compare'; title: string }
	| { id: string; kind: 'settings'; title: string }
	| { id: string; kind: 'history'; title: string }
	| { id: string; kind: 'ddl'; title: string; sql: string };

export function emptyDraft(engine: Engine = 'sqlite'): ConnectionDraft {
	return {
		id: null,
		name: '',
		engine,
		groupName: 'Default',
		envColor: 'development',
		host: engine === 'sqlite' ? null : '127.0.0.1',
		port:
			engine === 'postgresql'
				? 5432
				: engine === 'mysql' || engine === 'mariadb'
					? 3306
					: engine === 'mssql'
						? 1433
						: engine === 'mongodb'
							? 27017
							: null,
		database: engine === 'sqlite' ? null : '',
		username: engine === 'sqlite' ? null : '',
		password: '',
		filePath: engine === 'sqlite' ? '' : null,
		connectionUri: null,
		ssl: { mode: 'prefer', caPath: null, clientCertPath: null, clientKeyPath: null },
		ssh: {
			enabled: false,
			host: '',
			port: 22,
			username: '',
			authMethod: 'password',
			privateKeyPath: null,
			passwordSecretRef: null
		},
		extraOptions: {}
	};
}
