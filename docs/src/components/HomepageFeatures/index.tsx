import React from 'react';
import clsx from 'clsx';
import styles from './styles.module.css';

type FeatureItem = {
  title: string;
  Svg: React.ComponentType<React.ComponentProps<'svg'>>;
  description: JSX.Element;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Open source forever',
    Svg: require('@site/static/img/undraw_docusaurus_tree.svg').default,
    description: (
      <>
        Open source in OpenGeoCoding means collaboration, affordability, transparency, and adaptability, benefiting users and developers alike.
      </>
    ),
  },
  {
    title: 'Multiple sources',
    Svg: require('@site/static/img/undraw_docusaurus_mountain.svg').default,
    description: (
      <>
        Open geocoding harnesses multiple sources of data to provide the best results. It currently uses OpenAddresses, OpenStreetData, Geonames and Who's on first.
      </>
    ),
  },
  {
    title: 'Powered by Rust & Golang',
    Svg: require('@site/static/img/rust.svg').default,
    description: (
      <>
        Importing data requires a lot of computing power, so we use Rust to make it fast through multithreading. We use Golang to make the API fast and easy to use.
      </>
    ),
  },
];

function Feature({title, Svg, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center">
        <Svg className={styles.featureSvg} role="img" />
      </div>
      <div className="text--center padding-horiz--md">
        <h3>{title}</h3>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures(): JSX.Element {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
