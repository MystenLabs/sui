function ExternalLink({
    href,
    label,
    className,
}: {
    href: string;
    label: string;
    className?: string;
}) {
    return (
        <a
            href={href}
            target="_blank"
            rel="noreferrer noopener"
            className={className}
        >
            {label}
        </a>
    );
}

export default ExternalLink;
