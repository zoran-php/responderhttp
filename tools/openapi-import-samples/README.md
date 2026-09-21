# OpenAPI import — manual test files

Files for trying the import by hand (PLAN.md Phase 8d). Click the import icon
at the top of the Collections sidebar (next to +) and pick one.

| File | Expected result |
|---|---|
| `pet-store-3.2.yaml` | Preview: YAML, OpenAPI 3.2, 8 requests, 5 saved responses. "By tag" is suggested and gives `store` (with `pets` and `orders` inside it) and `users`. "By path" gives `health`, `login`, `pets` (with `photo` inside it) and `store/orders`. The environment has `baseUrl` = `https://eu.petstore.example.com/v1`, `X-Request-Id`, `petId` = `42`, and `token` and `apiKey`, both secret and empty. Notes: QUERY left out, 1 webhook, the staging server, optional/cookie parameters, the multipart file field, the `4XX` status, oauth2 on "Delete a pet (deprecated)". |
| `tiny-3.0-bom.json` | Imports (the UTF-8 BOM is fine). One request, `GET {{baseUrl}}/ping`, and a note that the document names no server. |
| `invalid-3.1.json` | Refused: "The document is not valid OpenAPI". 8 problems, including `info: missing properties 'title'` and `components › schemas › Pet › type`. |
| `external-ref.yaml` | Refused: "The document refers to other files", listing `./common.yaml#/components/schemas/Pet`. |
| `swagger-2.json` | Refused: "Swagger 2.0 is not supported". |
| `unquoted-version.yaml` | Refused: "The OpenAPI version is not text". |
| `not-openapi.toml` | Refused: "This file is not JSON or YAML". |

After importing `pet-store-3.2.yaml`, check:

- **Collection:** a new collection "Pet Store (import sample)" appears and opens.
- **Environment:** an environment with the same name appears, but is not activated.
- **Auth:** "List pets" shows Bearer `{{token}}` on the Auth tab.
- **Query parameters:** its Params tab shows `limit=20` and `status=available`, read from the URL.
- **Secrets:** in DBeaver, `environment_variables` has `token` and `apiKey` with `secret = 1` and no value.
