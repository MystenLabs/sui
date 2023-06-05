// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate, useParams } from 'react-router-dom';
import { KioskItems } from '../components/Kiosk/KioskItems';

export default function SingleKiosk(): JSX.Element {
  const { kioskId } = useParams();
  const navigate = useNavigate();

  if (!kioskId) {
    navigate('/');
    return <></>;
  }

  return (
    <div className="container">
      <KioskItems kioskId={kioskId}></KioskItems>
    </div>
  );
}
