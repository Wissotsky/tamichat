---
label: API
icon: book
---

# Polycentric API Documentation

Polycentric uses a variety of API methods to provide a CDN-friendly API for querying events, as well as posting them. Polycentric uses Protocol Buffers for data serialization. The primary motivation for this is reduced bandwith, as well as a way to store serialized events in the database to verify their signatures. We highly recommend using the Polycentric core TypeScript library to interact with the API, but feel free to use any other method.

## Content APIs

The first set of APIs are used for querying and posting events. These are the most common APIs used by Polycentric clients. Polycentric essentially operates as an append-only log of events, and with simple APIs, requests can remain cachable for a long time.

### GET /events

Retrieves events for a specific system within given ranges. Used for fetching user content, posts, and system updates.

```protobuf
// Request Query Parameters
message Query {
    PublicKey system;  // Base64URL-encoded system public key
    RangesForSystem ranges;  // Base64URL-encoded ranges to fetch
    optional ModerationFilters moderation_filters;
}

// Response
message Events {
    repeated SignedEvent events = 1;  // Array of signed events
}
```

### POST /events

Posts new events to the server, or existing events new to a server. Used for creating new content, updates, or system changes.

```protobuf
// Request Body
message Events {
    repeated SignedEvent events = 1;  // Array of events to post
}

// Response: Empty 200 OK
```

### GET /query_references

Queries events that reference specific byte strings, known as references. To find replies, pass in a reference to a pointer. For references to topics, pass in the topic as a byte string.

```protobuf
message QueryReferencesRequest {
    Reference reference = 1;  // Reference to query
    optional bytes cursor = 2;  // Pagination cursor
    optional QueryReferencesRequestEvents request_events = 3;
    repeated QueryReferencesRequestCountLWWElementReferences count_lww_element_references = 4;
    repeated QueryReferencesRequestCountReferences count_references = 5;
    repeated bytes extra_byte_references = 6;
}

message Reference {
    uint64 reference_type = 1; // type 2 is a pointer, type 3 is a byte reference
    bytes  reference      = 2; // either a pointer protobuf, or a byte string (no protobuf)
}

message QueryReferencesResponse {
    repeated QueryReferencesResponseEventItem items = 1;  // Matching events
    repeated SignedEvent related_events = 2;  // Related events
    optional bytes cursor = 3;  // Next page cursor
    repeated uint64 counts = 4;  // Reference counts
}
```

### GET /head

Returns the latest event for each process of a system.

#### Request

#### Query Parameters

- `system` (required): Base64URL-encoded public key of the system to fetch the head for
- `moderation_filters` (optional): Moderation filters to apply to the events

#### Example Request

```
GET /head?system=<base64url-encoded-public-key>
```

#### Response

Returns a Protocol Buffer encoded `Events` message containing:

1. The latest events (heads) for the system by logical clock, including the processes events

```protobuf
// Response: Latest events for the system
message Events {
    repeated SignedEvent events = 1;
}
```

#### Description

Retrieves a paginated list of events for a specific system and content type, along with proof events that verify the integrity of the event timeline.

#### Query Parameters

| Parameter          | Type   | Required | Description                                            |
| ------------------ | ------ | -------- | ------------------------------------------------------ |
| system             | string | Yes      | The public key of the system, URL-safe base64 encoded  |
| content_type       | number | Yes      | The type of content to query                           |
| limit              | number | No       | Maximum number of events to return (default: 10)       |
| after              | number | No       | Cursor for pagination - unix timestamp in milliseconds |
| moderation_filters | object | No       | Moderation filters to apply to the results             |

#### Response

Returns a protobuf-encoded response containing:

```protobuf
message QueryIndexResponse {
  repeated SignedEvent events = 1;
  repeated SignedEvent proof = 2;
}
```

#### Fields

- `events`: Array of signed events matching the query criteria, ordered by timestamp descending
- `proof`: Array of proof events that verify the timeline integrity

#### Notes

1. The response includes proof events to maintain timeline integrity across multiple processes within a system. For each process that isn't the source of the latest/earliest returned events, the API includes:

   - The next event after the latest returned event
   - The previous event before the earliest returned event

2. Events are ordered by:

   - Unix timestamp (descending)
   - Process ID (descending)
   - Logical clock (descending)

### GET /search

Performs a full-text search across events. Used for content discovery.

```protobuf
// Request Query Parameters
// - query: Search text
// - moderation_filters: Optional moderation filters for the events

// Response
message ResultEventsAndRelatedEventsAndCursor {
    Events result_events = 1;          // Matching events
    Events related_events = 2;         // Related context events
    optional bytes cursor = 3;         // Pagination cursor
}
```

### GET /explore

Returns trending or recommended content. Used for content discovery.

```protobuf
// Request Query Parameters
// - limit: Optional result limit
// - moderation_filters: Optional moderation filters for the events

// Response
message Events {
    repeated SignedEvent events = 1;  // Trending events
}
```

### GET /recommended_profiles

Returns recommended user profiles based on user interactions and network.

```protobuf
// Request Query Parameters
// - moderation_filters: Optional moderation filters for the events

// Response
message Events {
    repeated SignedEvent events = 1;  // Profile events
}
```

### GET /version

Returns server version information.

```protobuf
// Response: {"sha":"commithashhere"}
```

### POST /censor (Admin Only)

Applies moderation actions to content. Requires administrative access.

```protobuf
// Request Headers
// - Authorization: Admin token

// Request Query Parameters
// Censorship parameters for content moderation

// Response: Empty 200 OK on success
```

## FUTO ID APIs

### GET /claim_to_system

The `/claim_to_system` endpoint resolves identity claims to system identifiers in the Polycentric network. It enables searching for and verifying claims made by systems (users) that have been vouched for by trusted authorities.

#### Endpoint

```
GET /claim_to_system?query={base64_encoded_request}
```

#### Request Parameters

The request is base64-encoded and contains a `QueryClaimToSystemRequest` protobuf message:

```protobuf
message QueryClaimToSystemRequest {
    uint64 claim_type = 1;      // Type of claim to search for
    PublicKey trust_root = 2;   // Trust root for verification
    oneof query {
        string match_any_field = 3;                        // Match any field value
        QueryClaimToSystemRequestMatchAll match_all_fields = 4;  // Match all specified fields
    }
}
```

#### Claim Types

| ID  | Platform   | Description         |
| --- | ---------- | ------------------- |
| 1   | HackerNews | HackerNews username |
| 2   | YouTube    | YouTube channel     |
| 3   | Odysee     | Odysee channel      |
| 4   | Rumble     | Rumble channel      |
| 5   | Twitter    | Twitter handle      |
| 6   | Bitcoin    | Bitcoin address     |
| 7   | Generic    | Generic claim       |
| 8   | Discord    | Discord username    |
| ... | ...        | ...                 |

_See [full list of claim types](https://gitlab.futo.org/polycentric/polycentric/-/blob/master/packages/polycentric-core/src/models/index.ts#L495)_

#### Response

```protobuf
message QueryClaimToSystemResponse {
    repeated QueryClaimToSystemResponseMatch matches = 1;
}

message QueryClaimToSystemResponseMatch {
    SignedEvent claim = 1;                // The matching claim event
    repeated SignedEvent proof_chain = 2; // Chain of vouch events validating the claim
}
```

#### Notes

- Claims must be vouched for by the specified trust root to be included in results
- Results are ordered by vouch timestamp (most recent first)
- Match any field performs a case-sensitive search across all claim fields

### POST /claim_handle

Claims a handle for a system. Used for username registration.

```protobuf
message ClaimHandleRequest {
    PublicKey system = 1;  // System claiming the handle
    string handle = 2;  // Handle being claimed
}

// Response: Empty 200 OK on success
```

### GET /resolve_handle

Resolves a human-readable handle (username) to a system's public key. This endpoint is used to look up users by their username and get their system identifier.

#### Query Parameters

| Parameter | Type   | Required | Description             |
| --------- | ------ | -------- | ----------------------- |
| handle    | string | Yes      | The username to resolve |

#### Response

Returns a protobuf-encoded `PublicKey` representing the system associated with the handle.

```protobuf
message PublicKey {
  oneof key {
    bytes ed25519 = 1;
    // ... other key types
  }
}
```

#### Notes

1. Handles are unique within a server instance
2. Handles can only contain alphanumeric characters, underscores, and hyphens

### GET /find_claim_and_vouch

Finds claims and vouches for a system. Used for identity verification and trust chains.

```protobuf
message FindClaimAndVouchRequest {
    PublicKey vouching_system = 1;  // System that vouched
    PublicKey claiming_system = 2;  // System being vouched for
    repeated ClaimFieldEntry fields = 3;  // Fields to match
    uint64 claim_type = 4;  // Type of claim
}

message FindClaimAndVouchResponse {
    SignedEvent vouch = 1;  // Vouch event
    SignedEvent claim = 2;  // Claim event
}
```

### GET /challenge

Generates an authentication challenge for secure operations.

```protobuf
message HarborChallengeResponseBody {
    bytes challenge = 1;      // Challenge token
    uint64 created_on = 2;    // Challenge creation timestamp
}

message HarborChallengeResponse {
    bytes body = 1;           // Challenge body
    bytes hmac = 2;           // Challenge HMAC
}
```

### GET /resolve_claim

Resolves claims made by systems that are vouched for by a trusted root. This endpoint allows querying for claims of a specific type that match either any field or all specified fields.

#### Query Parameters

| Parameter | Type                       | Required | Description                                                    |
| --------- | -------------------------- | -------- | -------------------------------------------------------------- |
| query     | base64url-encoded protobuf | Yes      | A base64url-encoded QueryClaimToSystemRequest protobuf message |

#### Request Protobuf Structure

```protobuf
message QueryClaimToSystemRequest {
    uint64 claim_type = 1;
    PublicKey trust_root = 2;
    oneof query {
        string match_any_field = 3;
        ClaimFields match_all_fields = 4;
    }
}

## Notes on CDNs

Polycentric is designed to be used as a CDN for events. This means that the API is designed to be stateless and cacheable. The API is designed to be used in conjunction with a CDN, and the API is designed to be used in conjunction with a CDN.

The only cache invalidation required is when a user deletes a post, or if more recent counts are desired. We use Cloudflare for FUTO instances, however any CDN or web server can be used to serve the API.
