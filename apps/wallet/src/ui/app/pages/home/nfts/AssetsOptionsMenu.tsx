import { Menu } from '@headlessui/react';
import { Ooo24 } from '@mysten/icons';
import { Link } from '_src/ui/app/shared/Link';

const AssetsOptionsMenu = () => {
	return (
		<Menu>
			<Menu.Button
				className="cursor-pointer appearance-none border"
				style={{ border: 'none', background: 'none' }}
			>
				<Ooo24 />
			</Menu.Button>
			<Menu.Items className="absolute top-3 right-0 mt-2 w-40 bg-white border border-gray-90 divide-y divide-gray-200 rounded shadow-lg z-50">
				<Menu.Item>
					{({ active }) => (
						<div className="py-2 hover:bg-sui-light">
							<Link
								to="/hidden-assets"
								color="suiDark"
								weight="semibold"
								text="View Hidden Assets"
							/>
						</div>
					)}
				</Menu.Item>
			</Menu.Items>
		</Menu>
	);
};

export default AssetsOptionsMenu;
