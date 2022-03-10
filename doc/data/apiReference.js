module.exports = {
  "openapi": "3.0.3",
  "info": {
    "title": "Sui API",
    "version": "0.1"
  },
  "servers": [
    {
      "url": "/"
    }
  ],
  "paths": {
    "/addresses": {
      "get": {
        "tags": [
          "wallet"
        ],
        "description": "\nRetrieve all managed addresses for this client.",
        "operationId": "get_addresses",
        "responses": {
          "200": {
            "description": "successful operation",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/GetAddressResponse"
                }
              }
            }
          }
        }
      }
    },
    "/call": {
      "post": {
        "tags": [
          "wallet"
        ],
        "description": "\nExecute a Move call transaction by calling the specified function in the\nmodule of the given package. Arguments are passed in and type will be \ninferred from function signature. Gas usage is capped by the gas_budget.\n\nExample CallRequest\n{\n    \"sender\": \"b378b8d26c4daa95c5f6a2e2295e6e5f34371c1659e95f572788ffa55c265363\",\n    \"package_object_id\": \"0x2\",\n    \"module\": \"ObjectBasics\",\n    \"function\": \"create\",\n    \"args\": [\n        200,\n        \"b378b8d26c4daa95c5f6a2e2295e6e5f34371c1659e95f572788ffa55c265363\"\n    ],\n    \"gas_object_id\": \"1AC945CA31E77991654C0A0FCA8B0FD9C469B5C6\",\n    \"gas_budget\": 2000\n}",
        "operationId": "call",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/CallRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "successful operation",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TransactionResponse"
                }
              }
            }
          }
        }
      }
    },
    "/object_info": {
      "get": {
        "tags": [
          "wallet"
        ],
        "description": "\nReturns the object information for a specified object.",
        "operationId": "object_info",
        "parameters": [
          {
            "name": "objectId",
            "in": "query",
            "required": true,
            "style": "form",
            "explode": true,
            "schema": {
              "type": "string",
              "description": "Required; Hex code as string representing the object id"
            }
          },
          {
            "name": "owner",
            "in": "query",
            "required": false,
            "style": "form",
            "explode": true,
            "schema": {
              "type": "string",
              "description": "Optional; Hex code as string representing the owner's address",
              "nullable": true
            }
          }
        ],
        "responses": {
          "200": {
            "description": "successful operation",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ObjectInfoResponse"
                }
              }
            }
          }
        }
      }
    },
    "/objects": {
      "get": {
        "tags": [
          "wallet"
        ],
        "description": "\nReturns list of objects owned by an address.",
        "operationId": "get_objects",
        "parameters": [
          {
            "name": "address",
            "in": "query",
            "required": false,
            "style": "form",
            "explode": true,
            "schema": {
              "type": "string",
              "description": "Required; Hex code as string representing the address"
            }
          },
          {
            "name": "limit",
            "in": "query",
            "required": false,
            "style": "form",
            "explode": true,
            "schema": {
              "minimum": 1,
              "type": "integer",
              "description": "Maximum number of items returned by a single call",
              "format": "uint32",
              "nullable": true
            }
          },
          {
            "name": "page_token",
            "in": "query",
            "required": false,
            "style": "form",
            "explode": true,
            "schema": {
              "type": "string",
              "description": "Token returned by previous call to retreive the subsequent page",
              "nullable": true
            }
          }
        ],
        "responses": {
          "200": {
            "description": "successful operation",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ObjectResultsPage"
                }
              }
            }
          }
        },
        "x-dropshot-pagination": true
      }
    },
    "/sui/genesis": {
      "post": {
        "tags": [
          "debug"
        ],
        "description": "\nSpecify the genesis state of the network. \n\nYou can specify the number of authorities, an initial number of addresses \nand the number of gas objects to be assigned to those addresses.\n\nNote: This is a temporary endpoint that will no longer be needed once the \nnetwork has been started on testnet or mainnet.",
        "operationId": "genesis",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/GenesisRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "successful operation",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/GenesisResponse"
                }
              }
            }
          }
        }
      }
    },
    "/sui/start": {
      "post": {
        "tags": [
          "debug"
        ],
        "description": "\nStart servers with the specified configurations from the genesis endpoint.\n\nNote: This is a temporary endpoint that will no longer be needed once the \nnetwork has been started on testnet or mainnet.",
        "operationId": "sui_start",
        "responses": {
          "200": {
            "description": "successful operation"
          }
        }
      }
    },
    "/sui/stop": {
      "post": {
        "tags": [
          "debug"
        ],
        "description": "\nStop sui network and delete generated configs & storage.\n\nNote: This is a temporary endpoint that will no longer be needed once the \nnetwork has been started on testnet or mainnet.",
        "operationId": "sui_stop",
        "responses": {
          "204": {
            "description": "resource updated"
          }
        }
      }
    },
    "/sync": {
      "post": {
        "tags": [
          "wallet"
        ],
        "description": "\nSynchronize client state with authorities. This will fetch the latest information\non all objects owned by each address that is managed by this client state.",
        "operationId": "sync",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/SyncRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "204": {
            "description": "resource updated"
          }
        }
      }
    },
    "/transfer": {
      "post": {
        "tags": [
          "wallet"
        ],
        "description": "\nTransfer object from one address to another. Gas will be paid using the gas\nprovided in the request. This will be done through a native transfer \ntransaction that does not require Move VM executions, hence is much cheaper.\n\nNotes:\n- Non-coin objects cannot be transferred natively and will require a Move call\n\nExample TransferTransactionRequest\n{\n    \"from_address\": \"1DA89C9279E5199DDC9BC183EB523CF478AB7168\",\n    \"object_id\": \"4EED236612B000B9BEBB99BA7A317EFF27556A0C\",\n    \"to_address\": \"5C20B3F832F2A36ED19F792106EC73811CB5F62C\",\n    \"gas_object_id\": \"96ABE602707B343B571AAAA23E3A4594934159A5\"\n}",
        "operationId": "transfer_object",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/TransferTransactionRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "successful operation",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TransactionResponse"
                }
              }
            }
          }
        }
      }
    }
  },
  "components": {
    "schemas": {
      "CallRequest": {
        "required": [
          "args",
          "function",
          "gasBudget",
          "gasObjectId",
          "module",
          "packageObjectId",
          "sender"
        ],
        "type": "object",
        "properties": {
          "args": {
            "type": "array",
            "description": "Required; JSON representation of the arguments",
            "items": {}
          },
          "function": {
            "type": "string",
            "description": "Required; Name of the function to be called in the move module"
          },
          "gasBudget": {
            "minimum": 0,
            "type": "integer",
            "description": "Required; Gas budget required as a cap for gas usage",
            "format": "uint64"
          },
          "gasObjectId": {
            "type": "string",
            "description": "Required; Hex code as string representing the gas object id"
          },
          "module": {
            "type": "string",
            "description": "Required; Name of the move module"
          },
          "packageObjectId": {
            "type": "string",
            "description": "Required; Hex code as string representing Move module location"
          },
          "sender": {
            "type": "string",
            "description": "Required; Hex code as string representing the sender's address"
          }
        },
        "description": "Request containing the information required to execute a move module."
      },
      "GenesisRequest": {
        "type": "object",
        "properties": {
          "numAddresses": {
            "minimum": 0,
            "type": "integer",
            "description": "Optional; Number of managed addresses to be created at genesis",
            "format": "uint16",
            "nullable": true
          },
          "numAuthorities": {
            "minimum": 0,
            "type": "integer",
            "description": "Optional; Number of authorities to be started in the network",
            "format": "uint16",
            "nullable": true
          },
          "numGasObjects": {
            "minimum": 0,
            "type": "integer",
            "description": "Optional; Number of gas objects to be created for each address",
            "format": "uint16",
            "nullable": true
          }
        },
        "description": "Request containing the server configuration.\n\nAll attributes in GenesisRequest are optional, a default value will be used if the fields are not set."
      },
      "GenesisResponse": {
        "required": [
          "networkConfig",
          "walletConfig"
        ],
        "type": "object",
        "properties": {
          "networkConfig": {
            "description": "Information about authorities and the list of loaded move packages."
          },
          "walletConfig": {
            "description": "List of managed addresses and the list of authorities"
          }
        },
        "description": "Response containing the resulting wallet & network config of the provided genesis configuration."
      },
      "GetAddressResponse": {
        "required": [
          "addresses"
        ],
        "type": "object",
        "properties": {
          "addresses": {
            "type": "array",
            "description": "Vector of hex codes as strings representing the managed addresses",
            "items": {
              "type": "string"
            }
          }
        },
        "description": "Response containing the managed addresses for this client."
      },
      "Object": {
        "required": [
          "objectId",
          "objectRef"
        ],
        "type": "object",
        "properties": {
          "objectId": {
            "type": "string",
            "description": "Hex code as string representing the object id"
          },
          "objectRef": {
            "description": "Contains the object id, sequence number and object digest"
          }
        }
      },
      "ObjectInfoResponse": {
        "required": [
          "data",
          "id",
          "objType",
          "owner",
          "readonly",
          "version"
        ],
        "type": "object",
        "properties": {
          "data": {
            "description": "JSON representation of the object data"
          },
          "id": {
            "type": "string",
            "description": "Hex code as string representing the objet id"
          },
          "objType": {
            "type": "string",
            "description": "Type of object, i.e. Coin"
          },
          "owner": {
            "type": "string",
            "description": "Hex code as string representing the owner's address"
          },
          "readonly": {
            "type": "string",
            "description": "Boolean representing if the object is mutable"
          },
          "version": {
            "type": "string",
            "description": "Sequence number of the object"
          }
        },
        "description": "Response containing the information of an object if found, otherwise an error is returned."
      },
      "ObjectResultsPage": {
        "required": [
          "items"
        ],
        "type": "object",
        "properties": {
          "items": {
            "type": "array",
            "description": "list of items on this page of results",
            "items": {
              "$ref": "#/components/schemas/Object"
            }
          },
          "next_page": {
            "type": "string",
            "description": "token used to fetch the next page of results (if any)",
            "nullable": true
          }
        },
        "description": "A single page of results"
      },
      "SyncRequest": {
        "required": [
          "address"
        ],
        "type": "object",
        "properties": {
          "address": {
            "type": "string",
            "description": "Required; Hex code as string representing the address"
          }
        },
        "description": "Request containing the address that requires a sync."
      },
      "TransactionResponse": {
        "required": [
          "certificate",
          "objectEffectsSummary"
        ],
        "type": "object",
        "properties": {
          "certificate": {
            "description": "JSON representation of the certificate verifying the transaction"
          },
          "objectEffectsSummary": {
            "description": "JSON representation of the list of resulting effects on the object"
          }
        },
        "description": "Response containing the summary of effects made on an object and the certificate associated with the transaction that verifies the transaction."
      },
      "TransferTransactionRequest": {
        "required": [
          "fromAddress",
          "gasObjectId",
          "objectId",
          "toAddress"
        ],
        "type": "object",
        "properties": {
          "fromAddress": {
            "type": "string",
            "description": "Required; Hex code as string representing the address to be sent from"
          },
          "gasObjectId": {
            "type": "string",
            "description": "Required; Hex code as string representing the gas object id to be used as payment"
          },
          "objectId": {
            "type": "string",
            "description": "Required; Hex code as string representing the object id"
          },
          "toAddress": {
            "type": "string",
            "description": "Required; Hex code as string representing the address to be sent to"
          }
        },
        "description": "Request containing the information needed to execute a transfer transaction."
      }
    }
  }
}
