// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import {
	SIGNATURE_SCHEME_TO_FLAG,
	SignaturePubkeyPair,
	publicKeyFromSerialized,
	toB64,
	toParsedSignaturePubkeyPair,
} from '@mysten/sui.js';
import { AlertCircle } from 'lucide-react';
import { useState } from 'react';
import { Label } from '@/components/ui/label';
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@/components/ui/card';


import { useForm, useFieldArray, Controller, useWatch, FieldValues } from "react-hook-form";


let renderCount = 0;

export default function MultiSigAddress() {
  const { register, control, handleSubmit, reset, watch } = useForm({
    defaultValues: {
      test: [{ firstName: "Bill", lastName: "Luo" }]
    }
  });
  const {
    fields,
    append,
    prepend,
    remove,
    swap,
    move,
    insert,
    replace
  } = useFieldArray({
    control,
    name: "test"
  });

  const onSubmit = (data: FieldValues) => console.log("data", data);

  // if you want to control your fields with watch
  // const watchResult = watch("test");
  // console.log(watchResult);

  // The following is useWatch example
  // console.log(useWatch({ name: "test", control }));

  renderCount++;

  return (
    
		<div className="flex flex-col gap-4">
			<h2 className="scroll-m-20 text-4xl font-extrabold tracking-tight lg:text-5xl">
				MultiSig Address Creator
			</h2>
    
    
    <form 
      className="flex flex-col gap-4"
      onSubmit={handleSubmit(onSubmit)}>
      <p>The following demo allow you to create Sui MultiSig addresses.</p>
      <ul className="grid w-full gap-1.5">
        {fields.map((item, index) => {
          return (
            <li key={item.id}>
              <input
                {...register(`test.${index}.firstName`, { required: true })}
              />

              <Controller
                render={({ field }) => <input {...field} />}
                name={`test.${index}.lastName`}
                control={control}
              />
              <Button type="button" onClick={() => remove(index)}>
                Delete
              </Button>
            </li>
          );
        })}
      </ul>
      <section>
        <Button
          type="button"
          onClick={() => {
            append({ firstName: "appendBill", lastName: "appendLuo" });
          }}
        >
          append
        </Button>
        <Button
          type="button"
          onClick={() =>
            prepend({
              firstName: "prependFirstName",
              lastName: "prependLastName"
            })
          }
        >
          prepend
        </Button>
        <Button
          type="button"
          onClick={() =>
            insert(2, {
              firstName: "insertFirstName",
              lastName: "insertLastName"
            })
          }
        >
          insert at
        </Button>

        <Button type="button" onClick={() => swap(1, 2)}>
          swap
        </Button>

        <Button type="button" onClick={() => move(1, 2)}>
          move
        </Button>

        <Button
          type="button"
          onClick={() =>
            replace([
              {
                firstName: "test1",
                lastName: "test1"
              },
              {
                firstName: "test2",
                lastName: "test2"
              }
            ])
          }
        >
          replace
        </Button>

        <Button type="button" onClick={() => remove(1)}>
          remove at
        </Button>

        <Button
          type="button"
          onClick={() =>
            reset({
              test: [{ firstName: "Bill", lastName: "Luo" }]
            })
          }
        >
          reset
        </Button>
      </section>

      <input type="submit" />
    </form>
    </div>
  );
}

