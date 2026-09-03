/**
 * Holds the hand-rolled Lichess client in the Coach Engine to the official OpenAPI contract.
 *
 * The Coach Engine talks to Lichess through its own `reqwest` client and its own serde structs, so
 * nothing forces those to agree with what Lichess actually publishes. This check reads the two
 * sides and reports where they disagree:
 *
 *   - the Rust request builders, for the endpoint paths and query parameters they send;
 *   - `@lichess-org/types`, the generated TypeScript view of the official spec, for what exists.
 *
 * It exists because a serde struct once required a `turns` field that Lichess has never published.
 * Every Lichess Game silently failed to parse, and the Daily Coaching digest quietly shipped with
 * one provider missing. A required Rust field with no matching schema property is therefore an
 * error here, not a warning.
 */

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import ts from "typescript";

const LICHESS_ORIGIN = "https://lichess.org";

/** Rust files that build Lichess requests or decode Lichess responses. */
const RUST_SOURCES = [
  "services/coach-engine/src/profile_game_feed.rs",
  "services/coach-engine/src/profile_game_feed/window_probe.rs",
] as const;

/**
 * Which published schema each Rust struct decodes. Nothing in the Rust states this, so it is
 * authored here — and a struct named here that no longer exists in the source fails the check
 * rather than silently dropping its fields from coverage.
 */
const DECODED_SCHEMAS: ReadonlyArray<{ struct: string; schema: string }> = [
  { struct: "LichessWindowGame", schema: "GameJson" },
  { struct: "LichessProfileGame", schema: "GameJson" },
  { struct: "LichessPlayers", schema: "GamePlayers" },
  { struct: "LichessPlayer", schema: "GamePlayerUser" },
  { struct: "LichessUser", schema: "LightUser" },
  { struct: "LichessWindowClock", schema: "GameJson.clock" },
];

export type ConformanceFinding = {
  readonly severity: "error" | "warning";
  readonly kind:
    | "unpublishedPath"
    | "unpublishedQueryParameter"
    | "requiredFieldIsNotPublished"
    | "optionalFieldIsNotPublished";
  readonly detail: string;
};

export type SchemaProperty = { readonly optional: boolean };
export type Schema = ReadonlyMap<string, SchemaProperty>;

type PublishedApi = {
  readonly paths: ReadonlySet<string>;
  readonly queryParameters: ReadonlyMap<string, ReadonlySet<string>>;
  readonly schemas: ReadonlyMap<string, Schema>;
};

export type RustStruct = {
  readonly name: string;
  readonly fields: ReadonlyArray<{
    readonly name: string;
    readonly required: boolean;
  }>;
};

type RustSurface = {
  readonly requests: ReadonlyArray<{
    readonly path: string;
    readonly queryParameters: string[];
  }>;
  readonly structs: ReadonlyMap<string, RustStruct>;
};

/** Collapses `{username}` and the Rust `{}` placeholder to one shape so the two sides compare. */
function normalizePath(path: string): string {
  return path.replaceAll(/\{[^}]*\}/g, "{}");
}

function typeLiteralProperties(
  node: ts.TypeNode | undefined,
): Map<string, SchemaProperty> {
  const properties = new Map<string, SchemaProperty>();
  if (!node || !ts.isTypeLiteralNode(node)) {
    return properties;
  }
  for (const member of node.members) {
    if (!ts.isPropertySignature(member) || !member.name) {
      continue;
    }
    const name =
      ts.isIdentifier(member.name) || ts.isStringLiteral(member.name)
        ? member.name.text
        : undefined;
    if (name !== undefined) {
      properties.set(name, { optional: member.questionToken !== undefined });
    }
  }
  return properties;
}

function findInterface(
  source: ts.SourceFile,
  name: string,
): ts.InterfaceDeclaration {
  const declaration = source.statements.find(
    (statement): statement is ts.InterfaceDeclaration =>
      ts.isInterfaceDeclaration(statement) && statement.name.text === name,
  );
  if (!declaration) {
    throw new Error(
      `@lichess-org/types no longer declares "${name}"; the conformance check needs updating`,
    );
  }
  return declaration;
}

function memberType(
  members: ts.NodeArray<ts.TypeElement> | ts.TypeElement[],
  name: string,
): ts.TypeNode | undefined {
  for (const member of members) {
    if (!ts.isPropertySignature(member) || !member.name) {
      continue;
    }
    const memberName =
      ts.isIdentifier(member.name) || ts.isStringLiteral(member.name)
        ? member.name.text
        : undefined;
    if (memberName === name) {
      return member.type;
    }
  }
  return undefined;
}

/** Reads the generated spec view shipped by `@lichess-org/types`. */
export function readPublishedApi(): PublishedApi {
  const require = createRequire(import.meta.url);
  const declarationPath =
    require.resolve("@lichess-org/types/lichess-api.d.ts");
  const source = ts.createSourceFile(
    declarationPath,
    readFileSync(declarationPath, "utf8"),
    ts.ScriptTarget.ES2022,
    true,
  );
  const { paths, pathOperations } = readPublishedPathOperations(source);
  const operationQuery = readPublishedOperationQuery(source);
  const queryParameters = new Map<string, ReadonlySet<string>>();
  for (const [path, operationId] of pathOperations) {
    const query = operationQuery.get(operationId);
    if (query) {
      queryParameters.set(path, query);
    }
  }
  return {
    paths,
    queryParameters,
    schemas: readPublishedSchemas(source),
  };
}

function readPublishedPathOperations(source: ts.SourceFile) {
  const paths = new Set<string>();
  const pathOperations = new Map<string, string>();
  for (const member of findInterface(source, "paths").members) {
    if (
      !ts.isPropertySignature(member) ||
      !member.name ||
      !ts.isStringLiteral(member.name)
    ) {
      continue;
    }
    const path = member.name.text;
    paths.add(normalizePath(path));
    const operationId = publishedGetOperationId(member.type);
    if (operationId !== undefined) {
      pathOperations.set(normalizePath(path), operationId);
    }
  }
  return { paths, pathOperations };
}

/** `get: operations["apiGamesUser"]` */
function publishedGetOperationId(type: ts.TypeNode | undefined) {
  const getType =
    type && ts.isTypeLiteralNode(type)
      ? memberType(type.members, "get")
      : undefined;
  if (
    getType &&
    ts.isIndexedAccessTypeNode(getType) &&
    ts.isLiteralTypeNode(getType.indexType)
  ) {
    const literal = getType.indexType.literal;
    if (ts.isStringLiteral(literal)) {
      return literal.text;
    }
  }
  return undefined;
}

function readPublishedOperationQuery(source: ts.SourceFile) {
  const operationQuery = new Map<string, ReadonlySet<string>>();
  for (const member of findInterface(source, "operations").members) {
    if (!ts.isPropertySignature(member) || !member.name || !member.type) {
      continue;
    }
    const operationId = typeElementName(member);
    if (operationId === undefined || !ts.isTypeLiteralNode(member.type)) {
      continue;
    }
    const parameters = memberType(member.type.members, "parameters");
    const query =
      parameters && ts.isTypeLiteralNode(parameters)
        ? memberType(parameters.members, "query")
        : undefined;
    operationQuery.set(
      operationId,
      new Set(typeLiteralProperties(query).keys()),
    );
  }
  return operationQuery;
}

function readPublishedSchemas(source: ts.SourceFile) {
  const schemas = new Map<string, Schema>();
  const schemasType = memberType(
    findInterface(source, "components").members,
    "schemas",
  );
  if (!schemasType || !ts.isTypeLiteralNode(schemasType)) {
    throw new Error("@lichess-org/types no longer exposes components.schemas");
  }
  for (const member of schemasType.members) {
    readPublishedSchemaMember(member, schemas);
  }
  return schemas;
}

function readPublishedSchemaMember(
  member: ts.TypeElement,
  schemas: Map<string, Schema>,
) {
  if (!ts.isPropertySignature(member) || !member.name || !member.type) {
    return;
  }
  const name = typeElementName(member);
  if (name === undefined || !ts.isTypeLiteralNode(member.type)) {
    return;
  }
  schemas.set(name, typeLiteralProperties(member.type));
  // Inline object properties are addressable as `Schema.property`, e.g. `GameJson.clock`.
  for (const nested of member.type.members) {
    readPublishedNestedSchema(name, nested, schemas);
  }
}

function readPublishedNestedSchema(
  name: string,
  nested: ts.TypeElement,
  schemas: Map<string, Schema>,
) {
  if (!ts.isPropertySignature(nested) || !nested.name || !nested.type) {
    return;
  }
  const nestedName = typeElementName(nested);
  if (nestedName !== undefined && ts.isTypeLiteralNode(nested.type)) {
    schemas.set(`${name}.${nestedName}`, typeLiteralProperties(nested.type));
  }
}

function typeElementName(member: ts.TypeElement): string | undefined {
  if (!ts.isPropertySignature(member) || !member.name) {
    return undefined;
  }
  return ts.isIdentifier(member.name) || ts.isStringLiteral(member.name)
    ? member.name.text
    : undefined;
}

/** Converts a serde `rename_all = "camelCase"` field name to the wire name. */
function camelCase(field: string): string {
  return field.replaceAll(/_([a-z0-9])/g, (_match, letter: string) =>
    letter.toUpperCase(),
  );
}

export function readRustStructs(contents: string): Map<string, RustStruct> {
  const structs = new Map<string, RustStruct>();
  const pattern = /struct\s+(Lichess\w+)\s*\{([\s\S]*?)\n\}/g;
  for (const match of contents.matchAll(pattern)) {
    const [, name, body] = match;
    if (name === undefined || body === undefined) {
      continue;
    }
    const fields: Array<{ name: string; required: boolean }> = [];
    for (const line of body.split("\n")) {
      const field =
        /^\s*(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*(.+?),\s*$/.exec(
          line,
        );
      if (!field) {
        continue;
      }
      const [, fieldName, fieldType] = field;
      if (fieldName === undefined || fieldType === undefined) {
        continue;
      }
      fields.push({
        name: camelCase(fieldName),
        required: !fieldType.startsWith("Option<"),
      });
    }
    structs.set(name, { name, fields });
  }
  return structs;
}

function readRustRequests(contents: string): RustSurface["requests"] {
  const requests: Array<{ path: string; queryParameters: string[] }> = [];
  // Every Lichess URL the builders format, plus the `&until=` fragment appended for cursor paging.
  const urlPattern = new RegExp(`"${LICHESS_ORIGIN}(/[^"]*)"`, "g");
  const fragmentPattern = /"&([a-zA-Z][\w]*)=/g;

  const fragmentParameters = [...contents.matchAll(fragmentPattern)]
    .map(([, name]) => name)
    .filter((name): name is string => name !== undefined);

  for (const [, url] of contents.matchAll(urlPattern)) {
    if (url === undefined) {
      continue;
    }
    const [rawPath = "", rawQuery = ""] = url.split("?");
    // Profile pages and Game permalinks are canonical player-facing links, not API operations.
    if (!rawPath.startsWith("/api/")) {
      continue;
    }
    const queryParameters = [...rawQuery.matchAll(/[?&]?([a-zA-Z][\w]*)=/g)]
      .map(([, name]) => name)
      .filter((name): name is string => name !== undefined);
    requests.push({
      path: normalizePath(rawPath),
      // Fragments cannot be attributed to a single URL, so every builder in the file carries them.
      queryParameters: [
        ...new Set([
          ...queryParameters,
          ...(rawQuery ? fragmentParameters : []),
        ]),
      ],
    });
  }
  return requests;
}

/** Drops the trailing `mod tests` block so fixture URLs never count as production requests. */
function withoutTestModule(contents: string): string {
  const testModule = /#\[cfg\(test\)\]\s*\nmod tests\b/.exec(contents);
  return testModule ? contents.slice(0, testModule.index) : contents;
}

export function readRustSurface(repositoryRoot: string): RustSurface {
  const requests: Array<{ path: string; queryParameters: string[] }> = [];
  const structs = new Map<string, RustStruct>();
  for (const relativePath of RUST_SOURCES) {
    const contents = withoutTestModule(
      readFileSync(`${repositoryRoot}/${relativePath}`, "utf8"),
    );
    requests.push(...readRustRequests(contents));
    for (const [name, struct] of readRustStructs(contents)) {
      structs.set(name, struct);
    }
  }
  if (requests.length === 0) {
    throw new Error(
      "found no Lichess request URLs in the Coach Engine; the check cannot pass",
    );
  }
  return { requests, structs };
}

/** Reports every field a Rust struct decodes that its published schema does not declare. */
export function compareDecodedStruct(
  struct: RustStruct,
  schemaName: string,
  schema: Schema,
): ConformanceFinding[] {
  return struct.fields
    .filter((field) => !schema.has(field.name))
    .map((field) =>
      field.required
        ? {
            severity: "error" as const,
            kind: "requiredFieldIsNotPublished" as const,
            detail: `${struct.name} requires "${field.name}", which Lichess does not publish on ${schemaName}; every response will fail to decode`,
          }
        : {
            severity: "warning" as const,
            kind: "optionalFieldIsNotPublished" as const,
            detail: `${struct.name} reads optional "${field.name}", which Lichess does not publish on ${schemaName}`,
          },
    );
}

export function checkLichessApiConformance(
  repositoryRoot: string,
): ConformanceFinding[] {
  const published = readPublishedApi();
  const surface = readRustSurface(repositoryRoot);
  const findings: ConformanceFinding[] = [];

  for (const request of surface.requests) {
    if (!published.paths.has(request.path)) {
      findings.push({
        severity: "error",
        kind: "unpublishedPath",
        detail: `Lichess does not publish the path "${request.path}"`,
      });
      continue;
    }
    const declared = published.queryParameters.get(request.path);
    if (!declared) {
      continue;
    }
    for (const parameter of request.queryParameters) {
      if (!declared.has(parameter)) {
        findings.push({
          severity: "error",
          kind: "unpublishedQueryParameter",
          detail: `"${request.path}" does not accept the query parameter "${parameter}" we send`,
        });
      }
    }
  }

  for (const { struct, schema } of DECODED_SCHEMAS) {
    const decoded = surface.structs.get(struct);
    if (!decoded) {
      throw new Error(
        `${struct} no longer exists in the Coach Engine; update DECODED_SCHEMAS so its fields stay covered`,
      );
    }
    const publishedSchema = published.schemas.get(schema);
    if (!publishedSchema) {
      throw new Error(`Lichess no longer publishes the schema "${schema}"`);
    }
    findings.push(...compareDecodedStruct(decoded, schema, publishedSchema));
  }

  return findings;
}

export function formatFindings(
  findings: ReadonlyArray<ConformanceFinding>,
): string {
  if (findings.length === 0) {
    return "The Coach Engine's Lichess client matches every published path, parameter, and field.";
  }
  return findings
    .map(
      (finding) =>
        `${finding.severity === "error" ? "✗" : "!"} [${finding.kind}] ${finding.detail}`,
    )
    .join("\n");
}

if (import.meta.main) {
  const findings = checkLichessApiConformance(`${import.meta.dir}/../..`);
  console.log(formatFindings(findings));
  if (findings.some((finding) => finding.severity === "error")) {
    process.exit(1);
  }
}
