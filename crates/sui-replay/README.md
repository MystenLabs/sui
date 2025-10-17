The definition of InputObjectKind::SharedMoveObject changed recently - to update old sandbox files to conform to the new format, run the following command:

        jq -c  'walk(
          if type == "object" and has("SharedMoveObject") then
            .SharedMoveObject |= (
              if has("mutable") then
                . + {mutability: (if .mutable then "Mutable" else "Immutable" end)}
                | del(.mutable)
              else
                .
              end
            )
          elif type == "object" and has("SharedObject") then
            .SharedObject |= (
              if has("mutable") then
                . + {mutability: (if .mutable then "Mutable" else "Immutable" end)}
                | del(.mutable)
              else
                .
              end
            )
          else
            .
          end
        )' input.json > output.json
